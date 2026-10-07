//! Local protocol controls use the maintained MCP server and Flight SDK path.
use super::*;
use rmcp::{RoleServer, ServerHandler, model::*, service::RequestContext};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use veoveo_testing_support::lifecycle::owner;
const TASK: &str = "01983da0-0000-7000-8000-000000000001";
#[derive(Clone)]
struct Peer {
    created: Arc<AtomicUsize>,
    gets: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    wrong_cleanup_parent: bool,
    first_read_error: bool,
}
impl Peer {
    fn task(&self) -> Task {
        Task::new(
            TASK,
            TaskStatus::Working,
            "2026-10-06T00:00:00Z",
            "2026-10-06T00:00:00Z",
        )
        .with_poll_interval_ms(10)
    }
}
impl ServerHandler for Peer {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::default();
        config.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_tasks()
            .build();
        config
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        if request.name != "fixtureTask" {
            return Err(rmcp::ErrorData::invalid_params("wrong tool", None));
        }
        self.created.fetch_add(1, Ordering::SeqCst);
        Ok(CallToolResponse::Task(CreateTaskResult::new(self.task())))
    }
    async fn get_task(
        &self,
        request: GetTaskParams,
        _: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, rmcp::ErrorData> {
        if request.task_id != TASK {
            return Err(rmcp::ErrorData::invalid_params("wrong task", None));
        }
        let count = self.gets.fetch_add(1, Ordering::SeqCst);
        if self.first_read_error && count == 0 {
            return Err(rmcp::ErrorData::invalid_params(
                "fixture read refused",
                None,
            ));
        }
        let payload = if self.cancelled.load(Ordering::SeqCst) {
            TaskPayload::Cancelled
        } else {
            TaskPayload::Working
        };
        let mut task = self.task();
        // The first ordinary wait receives the correct acknowledged identity.
        // A later cleanup read cannot settle a different parent.
        if self.wrong_cleanup_parent && count > 0 {
            task.task_id = "foreign-task".into();
        }
        Ok(GetTaskResult::new(DetailedTask::new(task, payload)))
    }
    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        _: RequestContext<RoleServer>,
    ) -> Result<(), rmcp::ErrorData> {
        if request.task_id != TASK {
            return Err(rmcp::ErrorData::invalid_params(
                "wrong cancellation parent",
                None,
            ));
        }
        self.cancelled.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn actual_flight_task_wait_interruption_retains_and_cancels_the_acknowledged_peer_task() {
    const MODE: &str = "VEOVEO_TEST_FLIGHT_TASK_PEER";
    if let Ok(mode) = std::env::var(MODE) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let peer = Peer {
                created: Arc::new(AtomicUsize::new(0)),
                gets: Arc::new(AtomicUsize::new(0)),
                cancelled: Arc::new(AtomicBool::new(false)),
                wrong_cleanup_parent: mode == "wrong",
                first_read_error: false,
            };
            let factory = peer.clone();
            let service: rmcp::transport::streamable_http_server::StreamableHttpService<
                Peer,
                rmcp::transport::streamable_http_server::session::never::NeverSessionManager,
            > = rmcp::transport::streamable_http_server::StreamableHttpService::new(
                move || Ok(factory.clone()),
                Default::default(),
                Default::default(),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
            let router = axum::Router::new().nest_service("/mcp", service);
            let server = tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap();
            });
            let gets = peer.gets.clone();
            let signal = if mode == "signal" {
                Some(tokio::spawn(async move {
                    while gets.load(Ordering::SeqCst) == 0 {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    let status = std::process::Command::new("kill")
                        .args(["-INT", &std::process::id().to_string()])
                        .status()
                        .unwrap();
                    assert!(status.success());
                }))
            } else {
                None
            };
            let result: Result<Value> = owner::run(async {
                let session =
                    veoveo_testing_support::connect_mcp_client(&endpoint, "private-fixture-bearer")
                        .await?;
                run_owned_task(
                    &session,
                    "fixtureTask",
                    serde_json::json!({}),
                    Duration::from_secs(120),
                )
                .await
            })
            .await;
            assert!(result.is_err(), "interrupted Flight became successful");
            assert_eq!(
                peer.created.load(Ordering::SeqCst),
                1,
                "Task effect was not reached or was retried"
            );
            assert!(
                peer.gets.load(Ordering::SeqCst) >= 2,
                "acknowledged Task wait/cleanup was not reached"
            );
            assert_eq!(peer.cancelled.load(Ordering::SeqCst), mode != "wrong");
            let root = PathBuf::from(std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            let retained = root
                .read_dir()
                .unwrap()
                .filter_map(|entry| entry.ok())
                .find(|entry| entry.file_name().to_string_lossy().contains("unresolved"));
            assert_eq!(retained.is_some(), mode == "wrong");
            if let Some(retained) = retained {
                let receipt = std::fs::read_to_string(retained.path()).unwrap();
                assert!(receipt.contains(TASK));
                assert!(!receipt.contains("private-fixture-bearer"));
            }
            if let Some(signal) = signal {
                signal.await.unwrap();
            }
            server.abort();
            let _ = server.await;
        });
        return;
    }
    for mode in ["deadline", "signal", "wrong"] {
        let root = std::env::temp_dir().join(format!("flight-task-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            + if mode == "signal" {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(800)
            };
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "domain::client::tests::actual_flight_task_wait_interruption_retains_and_cancels_the_acknowledged_peer_task", "--nocapture"])
            .env(MODE, mode).env("VEOVEO_SMOKE_LOCAL_GROUPS", &root)
            .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.as_millis().to_string())
            .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "1").output().unwrap();
        assert!(
            output.status.success(),
            "actual Flight Task {mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[derive(Clone)]
struct StreamPeer {
    mode: String,
    starts: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
    reads: Arc<AtomicUsize>,
}
impl StreamPeer {
    fn view(&self) -> veoveo_stream_mcp::contract::LiveSessionView {
        super::super::tests::live_session(
            1,
            "pipeline",
            if self.stops.load(Ordering::SeqCst) > 0 {
                "stopped"
            } else {
                "running"
            },
        )
    }
}
impl ServerHandler for StreamPeer {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::default();
        config.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        config
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        use veoveo_stream_mcp::contract::{
            LiveSessionLifecycle, LiveStartDetails, PipelineId, StartLiveSessionOutput,
            StopLiveSessionOutput,
        };
        let view = self.view();
        let output = match request.name.as_ref() {
            "stream__start_live_session" => {
                self.starts.fetch_add(1, Ordering::SeqCst);
                if self.mode == "unknown" {
                    return std::future::pending().await;
                }
                let output = StartLiveSessionOutput::new(
                    view.session_id(),
                    PipelineId::parse(if self.mode == "foreign" {
                        "foreign"
                    } else {
                        "pipeline"
                    })
                    .unwrap(),
                    LiveStartDetails {
                        ingress: view.ingress.clone(),
                        video: view.video.clone(),
                        recording_output: None,
                        started_at: view.started_at.clone(),
                    },
                );
                serde_json::to_value(output).unwrap()
            }
            "stream__stop_live_session" => {
                if request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("sessionId"))
                    != Some(&serde_json::to_value(view.session_id()).unwrap())
                {
                    return Err(rmcp::ErrorData::invalid_params(
                        "wrong cleanup session",
                        None,
                    ));
                }
                self.stops.fetch_add(1, Ordering::SeqCst);
                if self.mode == "refused" {
                    return Err(rmcp::ErrorData::invalid_params("fixture refusal", None));
                }
                serde_json::to_value(StopLiveSessionOutput {
                    result_uri: veoveo_stream_mcp::uris::session_uri(view.session_id()),
                    lifecycle: LiveSessionLifecycle::Stopped,
                    received_video_frames: 12,
                    processed_frames: 12,
                    recording_output: None,
                    stopped_at: "2026-10-06T00:00:00Z".into(),
                })
                .unwrap()
            }
            _ => return Err(rmcp::ErrorData::invalid_params("wrong tool", None)),
        };
        Ok(CallToolResponse::Complete(CallToolResult::structured(
            output,
        )))
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, rmcp::ErrorData> {
        let output = if request.uri == veoveo_stream_mcp::uris::sessions_uri(None).as_str() {
            serde_json::to_value(veoveo_stream_mcp::contract::LiveSessionsPage {
                sessions: if self.mode == "reused" {
                    vec![self.view()]
                } else {
                    vec![]
                },
                limit: 100,
                next_cursor: None,
            })
            .unwrap()
        } else if request.uri
            == veoveo_stream_mcp::uris::session_uri(self.view().session_id()).to_string()
        {
            self.reads.fetch_add(1, Ordering::SeqCst);
            serde_json::to_value(self.view()).unwrap()
        } else {
            return Err(rmcp::ErrorData::invalid_params("wrong resource", None));
        };
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            serde_json::to_string(&output).unwrap(),
            request.uri,
        )])
        .into())
    }
}

#[test]
fn actual_flight_stream_start_interruption_preserves_registration_and_reused_session_ownership() {
    const MODE: &str = "VEOVEO_TEST_FLIGHT_STREAM_PEER";
    if let Ok(mode) = std::env::var(MODE) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let peer = StreamPeer {
                mode: mode.clone(),
                starts: Arc::new(AtomicUsize::new(0)),
                stops: Arc::new(AtomicUsize::new(0)),
                reads: Arc::new(AtomicUsize::new(0)),
            };
            let factory = peer.clone();
            let service: rmcp::transport::streamable_http_server::StreamableHttpService<
                StreamPeer,
                rmcp::transport::streamable_http_server::session::never::NeverSessionManager,
            > = rmcp::transport::streamable_http_server::StreamableHttpService::new(
                move || Ok(factory.clone()),
                Default::default(),
                Default::default(),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                axum::serve(listener, axum::Router::new().nest_service("/mcp", service))
                    .await
                    .unwrap();
            });
            let signal_reads = peer.reads.clone();
            let signal = if mode == "signal" {
                Some(tokio::spawn(async move {
                    while signal_reads.load(Ordering::SeqCst) == 0 {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    assert!(
                        std::process::Command::new("kill")
                            .args(["-INT", &std::process::id().to_string()])
                            .status()
                            .unwrap()
                            .success()
                    );
                }))
            } else {
                None
            };
            let result: Result<()> = owner::run(async {
                let pipeline = veoveo_stream_mcp::contract::PipelineId::parse("pipeline")?;
                let ordinary = super::super::cleanup::CleanupPeer::controlled(&endpoint, true);
                let live = super::super::stream::prepare_live_stream_pipeline_with_peer(
                    &ordinary,
                    &pipeline,
                    || async {
                        super::super::cleanup::StreamOwnership::register(
                            super::super::cleanup::CleanupPeer::controlled(&endpoint, false),
                            &endpoint,
                            &pipeline,
                        )
                    },
                )
                .await?;
                assert_eq!(live.owned_by_acceptance, mode != "reused");
                std::future::pending::<Result<()>>().await
            })
            .await;
            assert!(result.is_err(), "interrupted Flight became successful");
            assert_eq!(
                peer.starts.load(Ordering::SeqCst),
                usize::from(mode != "reused")
            );
            assert_eq!(
                peer.stops.load(Ordering::SeqCst),
                usize::from(!matches!(mode.as_str(), "reused" | "unknown" | "foreign"))
            );
            let root = PathBuf::from(std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            let retained = root
                .read_dir()
                .unwrap()
                .filter_map(|entry| entry.ok())
                .any(|entry| entry.file_name().to_string_lossy().contains("unresolved"));
            assert_eq!(
                retained,
                matches!(mode.as_str(), "unknown" | "foreign" | "refused")
            );
            if let Some(signal) = signal {
                signal.await.unwrap();
            }
            server.abort();
            let _ = server.await;
        });
        return;
    }
    for mode in [
        "deadline", "signal", "reused", "unknown", "foreign", "refused",
    ] {
        let root = std::env::temp_dir().join(format!("flight-stream-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            + if mode == "signal" {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(800)
            };
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "domain::client::tests::actual_flight_stream_start_interruption_preserves_registration_and_reused_session_ownership", "--nocapture"])
            .env(MODE, mode).env("VEOVEO_SMOKE_LOCAL_GROUPS", &root).env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.as_millis().to_string()).env("VEOVEO_SMOKE_CLEANUP_SECONDS", "1").output().unwrap();
        assert!(
            output.status.success(),
            "actual Flight Stream {mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[derive(Clone)]
struct VehiclePeer {
    takeoffs: Arc<AtomicUsize>,
    lands: Arc<AtomicUsize>,
    land_mode: String,
}
impl ServerHandler for VehiclePeer {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::default();
        config.capabilities = ServerCapabilities::builder().enable_tools().build();
        config
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        use veoveo_uav_sim_mcp::contract::{
            CommandAcknowledgement, SimulationState, UavResource, VehicleFlightState,
        };
        let mut state: SimulationState =
            serde_json::from_str(include_str!("../../../tests/fixtures/world-ready.json")).unwrap();
        if request
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.get("sessionId"))
            != Some(&serde_json::to_value(&state.session_id).unwrap())
        {
            return Err(rmcp::ErrorData::invalid_params("wrong session", None));
        }
        let output = match request.name.as_ref() {
            "uav-sim__takeoff_vehicle" | "uav-sim__land_vehicle" => {
                if request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("vehicleId"))
                    != Some(&serde_json::to_value(&state.vehicles[0].vehicle_id).unwrap())
                {
                    return Err(rmcp::ErrorData::invalid_params("wrong aircraft", None));
                }
                if request.name == "uav-sim__takeoff_vehicle" {
                    self.takeoffs.fetch_add(1, Ordering::SeqCst);
                } else {
                    self.lands.fetch_add(1, Ordering::SeqCst);
                }
                if request.name == "uav-sim__land_vehicle" {
                    if self.land_mode == "closed" {
                        return Err(rmcp::ErrorData::invalid_params(
                            "fixture dispatch failure",
                            None,
                        ));
                    }
                    if self.land_mode == "unknown" || self.land_mode == "cancelled" {
                        return std::future::pending().await;
                    }
                }
                let mut acknowledgement = serde_json::to_value(CommandAcknowledgement {
                    accepted: request.name != "uav-sim__land_vehicle"
                        || self.land_mode != "declined",
                    detail: "fixture acknowledgement".into(),
                    resource_uri: UavResource::Vehicle {
                        session: state.session_id.clone(),
                        vehicle: state.vehicles[0].vehicle_id.clone(),
                    },
                })
                .unwrap();
                if request.name == "uav-sim__land_vehicle" {
                    if self.land_mode == "foreign" {
                        acknowledgement["resourceUri"] =
                            serde_json::to_value(UavResource::Vehicle {
                                session: state.session_id.clone(),
                                vehicle: veoveo_uav_sim_mcp::contract::VehicleId::parse(
                                    "foreign-aircraft",
                                )
                                .unwrap(),
                            })
                            .unwrap();
                    }
                    if self.land_mode == "malformed" {
                        acknowledgement.as_object_mut().unwrap().remove("detail");
                    }
                }
                acknowledgement
            }
            "uav-sim__get_simulation_state" => {
                state.vehicles[0].flight_state =
                    if self.lands.load(Ordering::SeqCst) > 0 && self.land_mode == "accepted" {
                        VehicleFlightState::Landed
                    } else {
                        VehicleFlightState::Flying
                    };
                // The selected parent differs from an unrelated flying aircraft.
                let mut foreign = state.vehicles[0].clone();
                foreign.vehicle_id =
                    veoveo_uav_sim_mcp::contract::VehicleId::parse("foreign-aircraft").unwrap();
                foreign.flight_state = VehicleFlightState::Flying;
                state.vehicles.insert(0, foreign);
                serde_json::to_value(state).unwrap()
            }
            _ => return Err(rmcp::ErrorData::invalid_params("wrong vehicle tool", None)),
        };
        Ok(CallToolResponse::Complete(CallToolResult::structured(
            output,
        )))
    }
}

#[test]
fn actual_flight_takeoff_interruption_lands_only_its_acknowledged_vehicle() {
    const MODE: &str = "VEOVEO_TEST_FLIGHT_VEHICLE_PEER";
    if let Ok(mode) = std::env::var(MODE) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let peer = VehiclePeer {
                takeoffs: Arc::new(AtomicUsize::new(0)),
                lands: Arc::new(AtomicUsize::new(0)),
                land_mode: "accepted".into(),
            };
            let factory = peer.clone();
            let service: rmcp::transport::streamable_http_server::StreamableHttpService<
                VehiclePeer,
                rmcp::transport::streamable_http_server::session::never::NeverSessionManager,
            > = rmcp::transport::streamable_http_server::StreamableHttpService::new(
                move || Ok(factory.clone()),
                Default::default(),
                Default::default(),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                axum::serve(listener, axum::Router::new().nest_service("/mcp", service))
                    .await
                    .unwrap();
            });
            let takeoffs = peer.takeoffs.clone();
            let signal = if mode == "signal" {
                Some(tokio::spawn(async move {
                    while takeoffs.load(Ordering::SeqCst) == 0 {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    // Let the actual acknowledgement reach observed_identity first.
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    assert!(
                        std::process::Command::new("kill")
                            .args(["-INT", &std::process::id().to_string()])
                            .status()
                            .unwrap()
                            .success()
                    );
                }))
            } else {
                None
            };
            let result: Result<()> = owner::run(async {
                let scenario = super::super::UavAcceptanceScenario::load(
                    &super::super::tests::canonical_scenario(),
                )?;
                let ordinary = super::super::cleanup::CleanupPeer::controlled(&endpoint, true);
                let registered = super::super::cleanup::register_vehicle(
                    super::super::cleanup::CleanupPeer::controlled(&endpoint, false),
                    &endpoint,
                    &scenario,
                )?;
                super::super::cleanup::dispatch_takeoff(&ordinary, &scenario, &registered).await?;
                std::future::pending::<Result<()>>().await
            })
            .await;
            assert!(result.is_err());
            assert_eq!(peer.takeoffs.load(Ordering::SeqCst), 1);
            assert_eq!(peer.lands.load(Ordering::SeqCst), 1);
            let root = PathBuf::from(std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            assert!(
                !root
                    .read_dir()
                    .unwrap()
                    .filter_map(|entry| entry.ok())
                    .any(|entry| entry.file_name().to_string_lossy().contains("unresolved")),
                "selected vehicle cleanup did not settle"
            );
            if let Some(signal) = signal {
                signal.await.unwrap();
            }
            server.abort();
            let _ = server.await;
        });
        return;
    }
    for mode in ["deadline", "signal"] {
        let root =
            std::env::temp_dir().join(format!("flight-vehicle-peer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            + if mode == "signal" {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(800)
            };
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "domain::client::tests::actual_flight_takeoff_interruption_lands_only_its_acknowledged_vehicle", "--nocapture"])
            .env(MODE, mode).env("VEOVEO_SMOKE_LOCAL_GROUPS", &root).env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.as_millis().to_string()).env("VEOVEO_SMOKE_CLEANUP_SECONDS", "3").output().unwrap();
        assert!(
            output.status.success(),
            "actual Flight vehicle {mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[derive(Clone)]
struct FlightPeerFixture {
    task: Peer,
    stream: StreamPeer,
    vehicle: VehiclePeer,
    order: Arc<std::sync::Mutex<Vec<&'static str>>>,
}
impl ServerHandler for FlightPeerFixture {
    fn get_info(&self) -> ServerConfig {
        let mut config = self.task.get_info();
        config.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_tasks()
            .enable_resources()
            .build();
        config
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        match request.name.as_ref() {
            "fixtureTask" => self.task.call_tool(request, context).await,
            "stream__stop_live_session" => {
                self.order.lock().unwrap().push("stop-stream");
                self.stream.call_tool(request, context).await
            }
            "uav-sim__land_vehicle" => {
                self.order.lock().unwrap().push("land");
                self.vehicle.call_tool(request, context).await
            }
            "stream__start_live_session" => self.stream.call_tool(request, context).await,
            _ => self.vehicle.call_tool(request, context).await,
        }
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, rmcp::ErrorData> {
        self.stream.read_resource(request, context).await
    }
    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, rmcp::ErrorData> {
        self.task.get_task(request, context).await
    }
    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), rmcp::ErrorData> {
        self.order.lock().unwrap().push("cancel-task");
        self.task.cancel_task(request, context).await
    }
}

#[test]
fn inner_task_timeout_or_read_error_enters_reverse_cleanup_before_normal_postflight() {
    const MODE: &str = "VEOVEO_TEST_FLIGHT_INNER_ERROR";
    if let Ok(mode) = std::env::var(MODE) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let fixture = FlightPeerFixture {
                task: Peer {
                    created: Arc::new(AtomicUsize::new(0)),
                    gets: Arc::new(AtomicUsize::new(0)),
                    cancelled: Arc::new(AtomicBool::new(false)),
                    wrong_cleanup_parent: false,
                    first_read_error: mode == "read-error",
                },
                stream: StreamPeer {
                    mode: "deadline".into(),
                    starts: Arc::new(AtomicUsize::new(0)),
                    stops: Arc::new(AtomicUsize::new(0)),
                    reads: Arc::new(AtomicUsize::new(0)),
                },
                vehicle: VehiclePeer {
                    takeoffs: Arc::new(AtomicUsize::new(0)),
                    lands: Arc::new(AtomicUsize::new(0)),
                    land_mode: "accepted".into(),
                },
                order: Arc::new(std::sync::Mutex::new(Vec::new())),
            };
            let factory = fixture.clone();
            let service: rmcp::transport::streamable_http_server::StreamableHttpService<
                FlightPeerFixture,
                rmcp::transport::streamable_http_server::session::never::NeverSessionManager,
            > = rmcp::transport::streamable_http_server::StreamableHttpService::new(
                move || Ok(factory.clone()),
                Default::default(),
                Default::default(),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                axum::serve(listener, axum::Router::new().nest_service("/mcp", service))
                    .await
                    .unwrap();
            });
            let result: Result<Value> = owner::run(async {
                let scenario = super::super::UavAcceptanceScenario::load(
                    &super::super::tests::canonical_scenario(),
                )?;
                let pipeline = veoveo_stream_mcp::contract::PipelineId::parse("pipeline")?;
                let ordinary = super::super::cleanup::CleanupPeer::controlled(&endpoint, true);
                let live = super::super::stream::prepare_live_stream_pipeline_with_peer(
                    &ordinary,
                    &pipeline,
                    || async {
                        super::super::cleanup::StreamOwnership::register(
                            super::super::cleanup::CleanupPeer::controlled(&endpoint, false),
                            &endpoint,
                            &pipeline,
                        )
                    },
                )
                .await?;
                let vehicle = super::super::cleanup::register_vehicle(
                    super::super::cleanup::CleanupPeer::controlled(&endpoint, false),
                    &endpoint,
                    &scenario,
                )?;
                super::super::cleanup::dispatch_takeoff(&ordinary, &scenario, &vehicle).await?;
                let session =
                    veoveo_testing_support::connect_mcp_client(&endpoint, "private-fixture-bearer")
                        .await?;
                let flight = run_owned_task(
                    &session,
                    "fixtureTask",
                    serde_json::json!({}),
                    Duration::from_millis(40),
                )
                .await;
                // The actual production success-only postflight helper must
                // return this caller error before polling either ordinary path.
                super::super::cleanup::finish_flight(
                    flight,
                    || async {
                        vehicle.land(&ordinary, &scenario).await?;
                        vehicle.settled()
                    },
                    || async {
                        super::super::stream::stop_live_stream_session(
                            &ordinary,
                            &live.session_id,
                            "normal postflight",
                        )
                        .await?;
                        live.settled()
                    },
                )
                .await
            })
            .await;
            assert!(result.is_err());
            assert_eq!(fixture.task.created.load(Ordering::SeqCst), 1);
            assert!(fixture.task.cancelled.load(Ordering::SeqCst));
            assert_eq!(
                *fixture.order.lock().unwrap(),
                ["cancel-task", "land", "stop-stream"],
                "ordinary cleanup ran before retained Task cancellation"
            );
            let root = PathBuf::from(std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            assert!(
                !root
                    .read_dir()
                    .unwrap()
                    .filter_map(|entry| entry.ok())
                    .any(|entry| entry.file_name().to_string_lossy().contains("unresolved"))
            );
            server.abort();
            let _ = server.await;
        });
        return;
    }
    for mode in ["timeout", "read-error"] {
        let root = std::env::temp_dir().join(format!("flight-inner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            + Duration::from_secs(8);
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "domain::client::tests::inner_task_timeout_or_read_error_enters_reverse_cleanup_before_normal_postflight", "--nocapture"])
            .env(MODE, mode).env("VEOVEO_SMOKE_LOCAL_GROUPS", &root).env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.as_millis().to_string()).env("VEOVEO_SMOKE_CLEANUP_SECONDS", "4").output().unwrap();
        assert!(
            output.status.success(),
            "inner Flight {mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn landing_dispatch_is_shared_once_across_normal_and_interrupted_cleanup() {
    const MODE: &str = "VEOVEO_TEST_FLIGHT_LAND_ACK";
    if let Ok(mode) = std::env::var(MODE) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let peer = VehiclePeer {
                takeoffs: Arc::new(AtomicUsize::new(0)),
                lands: Arc::new(AtomicUsize::new(0)),
                land_mode: mode.clone(),
            };
            let factory = peer.clone();
            let service: rmcp::transport::streamable_http_server::StreamableHttpService<
                VehiclePeer,
                rmcp::transport::streamable_http_server::session::never::NeverSessionManager,
            > = rmcp::transport::streamable_http_server::StreamableHttpService::new(
                move || Ok(factory.clone()),
                Default::default(),
                Default::default(),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                axum::serve(listener, axum::Router::new().nest_service("/mcp", service))
                    .await
                    .unwrap();
            });
            let attempted = peer.lands.clone();
            let signal = if mode == "cancelled" {
                Some(tokio::spawn(async move {
                    while attempted.load(Ordering::SeqCst) == 0 {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    assert!(
                        std::process::Command::new("kill")
                            .args(["-INT", &std::process::id().to_string()])
                            .status()
                            .unwrap()
                            .success()
                    );
                }))
            } else {
                None
            };
            let result: Result<()> = owner::run(async {
                let scenario = super::super::UavAcceptanceScenario::load(
                    &super::super::tests::canonical_scenario(),
                )?;
                let ordinary = super::super::cleanup::CleanupPeer::controlled(&endpoint, true);
                let owned = super::super::cleanup::register_vehicle(
                    super::super::cleanup::CleanupPeer::controlled(&endpoint, false),
                    &endpoint,
                    &scenario,
                )?;
                super::super::cleanup::dispatch_takeoff(&ordinary, &scenario, &owned).await?;
                owned.land(&ordinary, &scenario).await?;
                owned.settled()
            })
            .await;
            assert_eq!(
                result.is_ok(),
                mode == "accepted",
                "land acknowledgement result: {result:?}"
            );
            assert_eq!(peer.takeoffs.load(Ordering::SeqCst), 1);
            assert_eq!(
                peer.lands.load(Ordering::SeqCst),
                1,
                "normal/cleanup repeated an attempted land effect"
            );
            let root = PathBuf::from(std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            let retained = root
                .read_dir()
                .unwrap()
                .filter_map(|entry| entry.ok())
                .find(|entry| entry.file_name().to_string_lossy().contains("unresolved"));
            assert_eq!(
                retained.is_some(),
                mode != "accepted",
                "nonterminal physical obligation was lost"
            );
            if let Some(retained) = retained {
                let receipt = std::fs::read_to_string(retained.path()).unwrap();
                assert!(receipt.contains("cleanupCommand"));
                assert!(!receipt.contains("private-fixture-bearer"));
            }
            if let Some(signal) = signal {
                signal.await.unwrap();
            }
            server.abort();
            let _ = server.await;
        });
        return;
    }
    for mode in [
        "accepted",
        "declined",
        "foreign",
        "malformed",
        "closed",
        "unknown",
        "stale",
        "cancelled",
    ] {
        let root = std::env::temp_dir().join(format!("flight-land-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            + if matches!(mode, "unknown" | "stale") {
                Duration::from_millis(800)
            } else {
                Duration::from_secs(5)
            };
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "domain::client::tests::landing_dispatch_is_shared_once_across_normal_and_interrupted_cleanup", "--nocapture"])
            .env(MODE, mode).env("VEOVEO_SMOKE_LOCAL_GROUPS", &root).env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.as_millis().to_string()).env("VEOVEO_SMOKE_CLEANUP_SECONDS", "1").output().unwrap();
        assert!(
            output.status.success(),
            "land Flight {mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
