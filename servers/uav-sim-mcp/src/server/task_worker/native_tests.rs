//! Native HTTP/Store failure injection. These fixtures do not simulate flight or qualify GPUs.
use super::*;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use chrono::{TimeDelta, Utc};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{sync::Notify, task::JoinHandle};
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{PlatformStore, TaskStatus};
use veoveo_task_runtime::TaskRuntime;

use crate::{
    adapter::{Adapter, HttpAdapter},
    contract::{MissionPlanLifecycle, MissionResult},
    server::{
        catalog_tests::{grant, mission_request},
        control_authority::ControlAuthorityError,
        test_support::{self},
    },
};

#[derive(Clone, Copy, Debug)]
enum Reply {
    Completed,
    WrongMission,
    WrongKind,
    Incomplete,
    WrongCount,
    ReversedTime,
    Rejected,
    Redirect,
    Malformed,
    MissingRecording,
    InvalidRecordingKey,
}

struct ProviderState {
    reply: Reply,
    started: Notify,
    release: Notify,
    finished: Notify,
    calls: AtomicUsize,
    jobs: std::sync::Mutex<Vec<tokio::task::AbortHandle>>,
}

struct HttpFixture {
    url: url::Url,
    state: Arc<ProviderState>,
    server: JoinHandle<()>,
}

impl HttpFixture {
    async fn new(reply: Reply) -> Self {
        let state = Arc::new(ProviderState {
            reply,
            started: Notify::new(),
            release: Notify::new(),
            finished: Notify::new(),
            calls: AtomicUsize::new(0),
            jobs: std::sync::Mutex::new(Vec::new()),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let router = Router::new()
            .route("/v1/operations", post(provider))
            .with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { url, state, server }
    }
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.state.release.notify_one();
        self.server.abort();
        for job in self.state.jobs.lock().unwrap().drain(..) {
            job.abort();
        }
    }
}

async fn provider(
    State(state): State<Arc<ProviderState>>,
    Json(operation): Json<DurableOperation>,
) -> axum::response::Response {
    // Like Python's asyncio.to_thread, this work survives a dropped HTTP handler.
    // The fixture owns every job and aborts any unfinished job during cleanup.
    let job = tokio::spawn(provider_operation(state.clone(), operation));
    state.jobs.lock().unwrap().push(job.abort_handle());
    job.await.unwrap()
}

async fn provider_operation(
    state: Arc<ProviderState>,
    operation: DurableOperation,
) -> axum::response::Response {
    state.calls.fetch_add(1, Ordering::SeqCst);
    state.started.notify_one();
    tokio::time::timeout(Duration::from_secs(90), state.release.notified())
        .await
        .unwrap();
    let DurableOperation::ExecuteMission(request) = operation else {
        panic!("fixture expects a mission")
    };
    let now = Utc::now();
    let mut result = MissionResult {
        mission_id: request.mission_id,
        lifecycle: crate::contract::MissionLifecycle::Completed,
        started_at: now,
        finished_at: now,
        completed_waypoints: request
            .vehicles
            .iter()
            .map(|v| v.waypoints.len() as u64)
            .sum(),
        recording_uris: Vec::new(),
    };
    match state.reply {
        Reply::WrongMission => {
            result.mission_id = crate::contract::MissionId::parse("unrelated-mission").unwrap()
        }
        Reply::Incomplete => result.lifecycle = crate::contract::MissionLifecycle::Running,
        Reply::WrongCount => result.completed_waypoints += 1,
        Reply::ReversedTime => result.finished_at = now - TimeDelta::seconds(1),
        _ => {}
    }
    let mut output = serde_json::to_value(result).unwrap();
    let object = output.as_object_mut().unwrap();
    object.remove("recording_uris");
    object.insert(
        "recording_keys".into(),
        match state.reply {
            Reply::MissingRecording => serde_json::json!(["missing-native-recording"]),
            Reply::InvalidRecordingKey => serde_json::json!(["invalid/producer/key"]),
            _ => serde_json::json!([]),
        },
    );
    let response = match state.reply {
        Reply::Rejected => (
            StatusCode::CONFLICT,
            "simulator timed out after accepting mission",
        )
            .into_response(),
        Reply::Redirect => (
            StatusCode::TEMPORARY_REDIRECT,
            [(axum::http::header::LOCATION, "/v1/operations")],
            "mutation redirect is unsupported",
        )
            .into_response(),
        Reply::Malformed => (StatusCode::OK, "{invalid-json").into_response(),
        Reply::WrongKind => Json(serde_json::json!({"result": "capture_dataset", "output": {
            "session_id": request.session_id, "elapsed_seconds": 1.0, "recording_keys": []
        }}))
        .into_response(),
        _ => {
            Json(serde_json::json!({"result": "execute_mission", "output": output})).into_response()
        }
    };
    state.finished.notify_one();
    response
}

struct MissionCase {
    http: HttpFixture,
    state: Arc<AppState>,
    pilot: GatewayInternalIdentity,
    plan: VehicleMissionPlan,
    task_id: TaskId,
    worker: tokio::task::AbortHandle,
    finished: Option<tokio::sync::oneshot::Receiver<()>>,
}

impl MissionCase {
    async fn start(store: &PlatformStore, reply: Reply, timeout: Duration) -> Self {
        let http = HttpFixture::new(reply).await;
        let adapter = Arc::new(Adapter::Http(Box::new(
            HttpAdapter::new(
                http.url.clone(),
                Duration::from_secs(2),
                timeout,
                "native-fixture".into(),
                store.clone(),
                "mission-observation",
            )
            .unwrap(),
        )));
        let state = test_support::state(store, adapter, "mission-worker");
        let pilot = test_support::identity(
            &uuid::Uuid::now_v7().to_string(),
            "operations",
            "pilot",
            &[],
        );
        state
            .control_authority
            .grant(&pilot, grant(&pilot, "grant"))
            .await
            .unwrap();
        let prepared = state
            .control_authority
            .prepare_plan(&pilot, mission_request("mission"))
            .await
            .unwrap();
        let plan = prepared;
        let draft = state
            .control_authority
            .prepare_execution(&pilot, &plan.plan_id, 0)
            .await
            .unwrap();
        let caller = PlaneCaller {
            bearer_token: "native-fixture".into(),
            identity: pilot.clone(),
            memberships: BTreeSet::new(),
        };
        let created = create_task(
            &state,
            &caller,
            crate::contract::UavTaskKind::ExecuteMission.name(),
            serde_json::to_value(ExecuteVehicleMissionPlanRequest {
                plan_id: plan.plan_id.clone(),
                expected_revision: 0,
            })
            .unwrap(),
            RecoveryClass::InterruptedIndeterminate,
            BTreeSet::from([task_link::retention_pin()]),
        )
        .await
        .unwrap();
        let (plan, guard) = state
            .control_authority
            .admit_execution(draft, &state.tasks, &created.snapshot)
            .await
            .unwrap();
        let task_id = created.snapshot.task_id;
        state
            .tasks
            .claim(task_id, TASK_LEASE_DURATION)
            .await
            .unwrap();
        let cancellation = CancellationToken::new();
        let (done, finished) = tokio::sync::oneshot::channel();
        let work_state = state.clone();
        let work_cancellation = cancellation.clone();
        let operation = mission_operation(&plan).unwrap();
        let join = tokio::spawn(async move {
            run_task(
                work_state,
                task_id,
                operation,
                Some(guard),
                work_cancellation,
            )
            .await;
            let _ = done.send(());
        });
        let worker = join.abort_handle();
        state
            .tasks
            .register_worker(task_id, cancellation, join)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), http.state.started.notified())
            .await
            .expect("adapter was not dispatched");
        Self {
            http,
            state,
            pilot,
            plan,
            task_id,
            worker,
            finished: Some(finished),
        }
    }

    async fn wait(&mut self) -> TaskSnapshot {
        tokio::time::timeout(Duration::from_secs(55), self.finished.take().unwrap())
            .await
            .expect("mission worker exceeded 55 seconds")
            .unwrap();
        self.state.tasks.get(self.task_id).await.unwrap().unwrap()
    }

    async fn assert_fenced(&self) {
        let plan = self
            .state
            .control_authority
            .visible_plan(&self.pilot, false, &self.plan.plan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(plan.state, MissionPlanLifecycle::Executing);
        assert_eq!(
            plan.revision, self.plan.revision,
            "unknown outcomes must not settle the plan"
        );
        let next = self
            .state
            .control_authority
            .prepare_plan(
                &self.pilot,
                mission_request(&uuid::Uuid::now_v7().to_string()),
            )
            .await
            .unwrap();
        let error = start_vehicle_mission_plan(
            self.state.clone(),
            PlaneCaller {
                bearer_token: "native-fixture".into(),
                identity: self.pilot.clone(),
                memberships: BTreeSet::new(),
            },
            ExecuteVehicleMissionPlanRequest {
                plan_id: next.plan_id,
                expected_revision: next.revision,
            },
            BTreeSet::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            ControlAuthorityError::VehicleBusy(next.vehicle_id.to_string()).to_string()
        );
    }
}

impl Drop for MissionCase {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

#[tokio::test]
async fn native_cancellation_keeps_vehicle_fenced_after_provider_continues() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(65), async {
        let mut case = MissionCase::start(&db.a, Reply::Completed, Duration::from_secs(60)).await;
        case.state.tasks.cancel(case.task_id).await.unwrap();
        let task = case.wait().await;
        assert_eq!(task.status, TaskStatus::Failed);
        assert!(task.retention_pins.contains(&task_link::retention_pin()));
        assert_eq!(task.error.unwrap().code, "interrupted_indeterminate");
        case.assert_fenced().await;
        reconcile_mission_retention(&case.state).await.unwrap();
        assert!(
            case.state
                .tasks
                .get(case.task_id)
                .await
                .unwrap()
                .unwrap()
                .retention_pins
                .contains(&task_link::retention_pin())
        );
        case.http.state.release.notify_one();
        tokio::time::timeout(Duration::from_secs(5), case.http.state.finished.notified())
            .await
            .unwrap();
        case.assert_fenced().await;
        let recovered = TaskRuntime::new(db.b.clone(), "uav-sim", "replacement")
            .recover()
            .await
            .unwrap();
        assert!(recovered.resumable.is_empty());
        assert_eq!(case.http.state.calls.load(Ordering::SeqCst), 1);
    })
    .await
    .expect("cancellation qualification exceeded 65 seconds");
}

#[tokio::test]
async fn native_unconfirmed_responses_and_timeout_preserve_vehicle_authority() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(90), async {
        for reply in [
            Reply::WrongMission,
            Reply::WrongKind,
            Reply::Incomplete,
            Reply::WrongCount,
            Reply::ReversedTime,
            Reply::Rejected,
            Reply::Redirect,
            Reply::Malformed,
        ] {
            let mut case = MissionCase::start(&db.a, reply, Duration::from_secs(10)).await;
            case.http.state.release.notify_one();
            let task = case.wait().await;
            assert_eq!(task.status, TaskStatus::Failed, "{reply:?}");
            assert_eq!(
                task.error.unwrap().code,
                "interrupted_indeterminate",
                "{reply:?}"
            );
            case.assert_fenced().await;
            assert_eq!(case.http.state.calls.load(Ordering::SeqCst), 1, "{reply:?}");
        }
        let mut case = MissionCase::start(&db.a, Reply::Completed, Duration::from_secs(1)).await;
        assert_eq!(
            case.wait().await.error.unwrap().code,
            "interrupted_indeterminate"
        );
        case.assert_fenced().await;
    })
    .await
    .expect("unknown completion qualification exceeded 90 seconds");
}

#[tokio::test]
async fn native_completion_releases_vehicle_even_when_recording_projection_fails() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(65), async {
        for reply in [
            Reply::Completed,
            Reply::MissingRecording,
            Reply::InvalidRecordingKey,
        ] {
            let mut case = MissionCase::start(&db.a, reply, Duration::from_secs(10)).await;
            case.http.state.release.notify_one();
            let task = case.wait().await;
            if matches!(reply, Reply::Completed) {
                assert_eq!(task.status, TaskStatus::Succeeded);
            } else {
                assert_eq!(task.error.unwrap().code, "uav_sim_result_unavailable");
            }
            let plan = case
                .state
                .control_authority
                .visible_plan(&case.pilot, false, &case.plan.plan_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(plan.state, MissionPlanLifecycle::Completed);
            let next = case
                .state
                .control_authority
                .prepare_plan(&case.pilot, mission_request("next"))
                .await
                .unwrap();
            let (_, guard) = admit(&case.state, &case.pilot, &next).await.unwrap();
            case.state
                .control_authority
                .abort_execution(&guard)
                .await
                .unwrap();
        }
    })
    .await
    .expect("completion and catalog qualification exceeded 65 seconds");
}

#[tokio::test]
async fn native_task_lease_loss_cannot_release_vehicle_authority() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(65), async {
        let mut case = MissionCase::start(&db.a, Reply::Completed, Duration::from_secs(60)).await;
        db.b.client().query(include_str!("../../../queries/server/task_worker/native_tests/native_task_lease_loss_cannot_release_vehicle_authority.surql"))
            .bind(("task", veoveo_platform_store::task_record_id(case.task_id))).await.unwrap().check().unwrap();
        let task = case.wait().await;
        assert_eq!(task.status, TaskStatus::Running, "stale worker cannot settle the Task");
        case.assert_fenced().await;
        let recovery = TaskRuntime::new(db.b.clone(), "uav-sim", "replacement").recover().await.unwrap();
        assert_eq!(recovery.failed_indeterminate.len(), 1);
        case.assert_fenced().await;
        assert_eq!(case.http.state.calls.load(Ordering::SeqCst), 1);
    }).await.expect("lease loss qualification exceeded 65 seconds");
}

#[tokio::test]
async fn native_cancellation_after_physical_completion_preserves_settlement() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(65), async {
        for publication_race in [false, true] {
            let mut case =
                MissionCase::start(&db.a, Reply::MissingRecording, Duration::from_secs(10)).await;
            case.http.state.release.notify_one();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let plan = case
                        .state
                        .control_authority
                        .visible_plan(&case.pilot, false, &case.plan.plan_id)
                        .await
                        .unwrap()
                        .unwrap();
                    if plan.state == MissionPlanLifecycle::Completed {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("physical completion did not settle before catalog lookup");
            case.state.tasks.cancel(case.task_id).await.unwrap();
            let task = if publication_race {
                // The physical receipt is retained, but cancellation commits before the Task result.
                case.worker.abort();
                tokio::time::timeout(Duration::from_secs(5), case.finished.take().unwrap())
                    .await
                    .unwrap()
                    .unwrap_err();
                transition(
                    &case.state,
                    case.task_id,
                    TaskTransition::Succeeded {
                        message: "completed".into(),
                        result: serde_json::json!({}),
                    },
                )
                .await;
                case.state.tasks.get(case.task_id).await.unwrap().unwrap()
            } else {
                case.wait().await
            };
            assert_eq!(task.error.unwrap().code, "completed_after_cancellation");
            let plan = case
                .state
                .control_authority
                .visible_plan(&case.pilot, false, &case.plan.plan_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(plan.state, MissionPlanLifecycle::Completed);
        }
    })
    .await
    .expect("completed cancellation qualification exceeded 65 seconds");
}

#[tokio::test]
async fn native_failed_completion_transaction_keeps_the_vehicle_fenced() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(65), async {
        let mut case = MissionCase::start(&db.a, Reply::Completed, Duration::from_secs(10)).await;
        db.b.client().query(include_str!("../../../queries/server/task_worker/native_tests/native_failed_completion_transaction_keeps_the_vehicle_fenced.surql"))
            .await.unwrap().check().unwrap();
        case.http.state.release.notify_one();
        assert_eq!(case.wait().await.error.unwrap().code, "mission_authority_finalization_failed");
        case.assert_fenced().await;
        assert_eq!(case.http.state.calls.load(Ordering::SeqCst), 1);
    }).await.expect("completion persistence qualification exceeded 65 seconds");
}

#[tokio::test]
async fn native_queued_mission_recovery_does_not_decode_or_replay_a_simulator_command() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(65), async {
        let http = HttpFixture::new(Reply::Completed).await;
        let adapter = Arc::new(Adapter::Http(Box::new(
            HttpAdapter::new(
                http.url.clone(),
                Duration::from_secs(2),
                Duration::from_secs(10),
                "native-fixture".into(),
                db.a.clone(),
                "mission-observation",
            )
            .unwrap(),
        )));
        let state = test_support::state(&db.a, adapter, "recovered-worker");
        let pilot = test_support::identity("queued-recovery", "operations", "pilot", &[]);
        state
            .control_authority
            .grant(&pilot, grant(&pilot, "grant"))
            .await
            .unwrap();
        let prepared = state
            .control_authority
            .prepare_plan(&pilot, mission_request("queued"))
            .await
            .unwrap();
        let plan = prepared;
        let draft = state
            .control_authority
            .prepare_execution(&pilot, &plan.plan_id, 0)
            .await
            .unwrap();
        let caller = PlaneCaller {
            bearer_token: "native-fixture".into(),
            identity: pilot.clone(),
            memberships: BTreeSet::new(),
        };
        let created = create_task(
            &state,
            &caller,
            crate::contract::UavTaskKind::ExecuteMission.name(),
            serde_json::to_value(ExecuteVehicleMissionPlanRequest {
                plan_id: plan.plan_id.clone(),
                expected_revision: 0,
            })
            .unwrap(),
            RecoveryClass::InterruptedIndeterminate,
            BTreeSet::from([task_link::retention_pin()]),
        )
        .await
        .unwrap();
        let (plan, guard) = state
            .control_authority
            .admit_execution(draft, &state.tasks, &created.snapshot)
            .await
            .unwrap();
        drop(guard); // Process loss before claim; the retained request is a public plan address.
        let recovery = state.tasks.recover().await.unwrap();
        assert_eq!(recovery.resumable.len(), 1);
        resume_queued_operation(
            state.clone(),
            recovery.resumable.into_iter().next().unwrap(),
        )
        .await
        .unwrap();
        let task = state
            .tasks
            .get(created.snapshot.task_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, TaskStatus::Failed);
        assert_eq!(task.error.unwrap().code, "interrupted_indeterminate");
        assert_eq!(http.state.calls.load(Ordering::SeqCst), 0);
        let retained = state
            .control_authority
            .visible_plan(&pilot, false, &plan.plan_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained.state, MissionPlanLifecycle::Executing);
        let next = state
            .control_authority
            .prepare_plan(&pilot, mission_request("next"))
            .await
            .unwrap();
        assert!(matches!(
            admit(&state, &pilot, &next).await,
            Err(ControlAuthorityError::VehicleBusy(_))
        ));
    })
    .await
    .expect("queued recovery qualification exceeded 65 seconds");
}

async fn admit(
    state: &AppState,
    pilot: &GatewayInternalIdentity,
    plan: &VehicleMissionPlan,
) -> Result<(VehicleMissionPlan, MissionExecutionGuard), ControlAuthorityError> {
    let draft = state
        .control_authority
        .prepare_execution(pilot, &plan.plan_id, plan.revision)
        .await?;
    let caller = PlaneCaller {
        bearer_token: "native-fixture".into(),
        identity: pilot.clone(),
        memberships: BTreeSet::new(),
    };
    let created = create_task(
        state,
        &caller,
        crate::contract::UavTaskKind::ExecuteMission.name(),
        serde_json::to_value(ExecuteVehicleMissionPlanRequest {
            plan_id: plan.plan_id.clone(),
            expected_revision: plan.revision,
        })
        .unwrap(),
        RecoveryClass::InterruptedIndeterminate,
        BTreeSet::from([task_link::retention_pin()]),
    )
    .await
    .unwrap();
    state
        .control_authority
        .admit_execution(draft, &state.tasks, &created.snapshot)
        .await
}

#[tokio::test]
async fn native_restart_releases_only_the_domain_pin_for_a_never_admitted_task() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(65), async {
        let http = HttpFixture::new(Reply::Completed).await;
        let adapter = Arc::new(Adapter::Http(Box::new(
            HttpAdapter::new(
                http.url.clone(),
                Duration::from_secs(1),
                Duration::from_secs(1),
                "native-fixture".into(),
                db.a.clone(),
                "native",
            )
            .unwrap(),
        )));
        let state = test_support::state(&db.a, adapter, "replacement");
        let pilot = test_support::identity("pre-admission", "operations", "pilot", &[]);
        state
            .control_authority
            .grant(&pilot, grant(&pilot, "grant"))
            .await
            .unwrap();
        let plan = state
            .control_authority
            .prepare_plan(&pilot, mission_request("never-admitted"))
            .await
            .unwrap();
        let caller = PlaneCaller {
            bearer_token: "native-fixture".into(),
            identity: pilot.clone(),
            memberships: BTreeSet::new(),
        };
        let external = TaskRetentionPin::new("native:consumer").unwrap();
        let task = create_task(
            &state,
            &caller,
            crate::contract::UavTaskKind::ExecuteMission.name(),
            serde_json::to_value(ExecuteVehicleMissionPlanRequest {
                plan_id: plan.plan_id.clone(),
                expected_revision: 0,
            })
            .unwrap(),
            RecoveryClass::InterruptedIndeterminate,
            BTreeSet::from([task_link::retention_pin(), external.clone()]),
        )
        .await
        .unwrap()
        .snapshot;
        // The process stopped after Task creation, before any admission or simulator request.
        let recovered = state.tasks.recover().await.unwrap();
        assert_eq!(recovered.resumable.len(), 1);
        resume_queued_operation(
            state.clone(),
            recovered.resumable.into_iter().next().unwrap(),
        )
        .await
        .unwrap();
        let failed = state.tasks.get(task.task_id).await.unwrap().unwrap();
        assert_eq!(failed.status, TaskStatus::Failed);
        assert_eq!(failed.retention_pins, BTreeSet::from([external.clone()]));
        // A lost acknowledgement after a terminal write is repaired independently of Task recovery.
        state
            .tasks
            .adopt_retention_pin_for_repair(task.task_id, &task_link::retention_pin())
            .await
            .unwrap();
        assert!(state.tasks.recover().await.unwrap().resumable.is_empty());
        reconcile_mission_retention(&state).await.unwrap();
        assert_eq!(
            state
                .tasks
                .get(task.task_id)
                .await
                .unwrap()
                .unwrap()
                .retention_pins,
            BTreeSet::from([external])
        );
        assert_eq!(
            state
                .control_authority
                .visible_plan(&pilot, false, &plan.plan_id)
                .await
                .unwrap()
                .unwrap(),
            plan
        );
        assert_eq!(http.state.calls.load(Ordering::SeqCst), 0);
    })
    .await
    .expect("pre-admission restart qualification exceeded 65 seconds");
}
