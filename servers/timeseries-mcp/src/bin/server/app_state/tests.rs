//! Native settlement only; no forecasting, capability redemption or installed calls.
use super::super::{ForecastTaskRequest, SERVER_SLUG, TASK_LEASE_DURATION};
use super::settlement;
use crate::tool_input_tests::fixture;
use std::{collections::BTreeSet, time::Duration};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskOwner, TaskPayloadState, TaskRuntime, TaskSnapshot,
    TaskTransition,
};
use veoveo_types::TaskId;

fn owner() -> TaskOwner {
    use veoveo_types::{
        AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId,
        TenantId, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
    };
    let principal = PrincipalId::parse("forecast-owner").unwrap();
    TaskOwner {
        principal_key: "forecast-owner".into(),
        principal_kind: veoveo_task_runtime::PrincipalKind::Service,
        issuer: "https://settlement.test".into(),
        subject: "forecast-owner".into(),
        profile: "operator".into(),
        tenant_key: Some("tenant-a".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::parse("forecast").unwrap(),
            tenant: TenantId::parse("tenant-a").unwrap(),
            membership: WorkContextMembershipLevel::Contributor,
            policy_revision: PolicyVersion::parse("test-1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: principal,
            },
        },
    }
}

async fn running(runtime: &TaskRuntime) -> TaskSnapshot {
    use veoveo_artifact_contract::{
        ArtifactTaskId, ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret,
        IssuedArtifactWriteCapability,
    };
    use veoveo_duckdb_mcp::contract::DuckDbTabularSource;
    use veoveo_timeseries_mcp::contract::{
        TimeseriesForecastHorizon, TimeseriesForecastRequest, TimeseriesTableMapping,
        TimeseriesTaskKind,
    };
    use veoveo_types::TaskTypeDefinition;
    let id = TaskId::new();
    let request = ForecastTaskRequest {
        input: TimeseriesForecastRequest::new(
            DuckDbTabularSource::InlineCsv {
                csv: "value\n1\n2\n".into(),
                filename: None,
                options: Default::default(),
            },
            TimeseriesTableMapping::new("value".parse().unwrap()),
            TimeseriesForecastHorizon::new(3).unwrap(),
        ),
        // This admitted fixture secret is never issued, redeemed or printed.
        artifact_write_capability: IssuedArtifactWriteCapability {
            capability_id: ArtifactWriteCapabilityId::new(),
            secret: ArtifactWriteCapabilitySecret::new(
                "native-settlement-not-an-issued-capability",
            )
            .unwrap(),
            task_id: ArtifactTaskId::try_from(id.as_uuid()).unwrap(),
            expires_at: chrono::Utc::now() + chrono::TimeDelta::hours(1),
        },
    };
    runtime
        .create(CreateTask {
            task_id: id,
            owner: owner(),
            server: SERVER_SLUG.into(),
            task_type: TimeseriesTaskKind::Forecast.name(),
            request: serde_json::to_value(request).unwrap(),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    runtime
        .claim(id, TASK_LEASE_DURATION)
        .await
        .unwrap()
        .snapshot
}

fn success() -> TaskTransition {
    TaskTransition::Succeeded {
        result_uri: None,
        message: "native forecast settlement".into(),
        result: serde_json::json!({"nativeSettlement":true}),
    }
}

#[tokio::test]
async fn remote_cancellation_winning_forecast_settlement_is_terminal() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let executor = TaskRuntime::new(db.a.clone(), SERVER_SLUG, "forecast-executor");
        let remote = TaskRuntime::new(db.b.clone(), SERVER_SLUG, "forecast-canceller");
        let before = running(&executor).await;
        remote.cancel(before.task_id).await.unwrap();
        settlement::settle(&executor, &before, success())
            .await
            .unwrap();
        let after = remote.get(before.task_id).await.unwrap().unwrap();
        assert_eq!(after.task_id, before.task_id);
        assert_eq!(after.owner, before.owner);
        assert_eq!(after.request, before.request);
        assert_eq!(after.created_at, before.created_at);
        assert_eq!(
            executor.payload_state(before.task_id).await.unwrap(),
            TaskPayloadState::Cancelled
        );
        assert!(after.result.is_none());
    })
    .await
    .expect("Timeseries settlement qualification exceeded 60 seconds");
}

#[tokio::test]
async fn forecast_settlement_preserves_completion_and_cancellation_execution_fences() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let executor = TaskRuntime::new(db.a.clone(), SERVER_SLUG, "forecast-executor");
        let remote = TaskRuntime::new(db.b.clone(), SERVER_SLUG, "forecast-canceller");

        let before = running(&executor).await;
        remote.cancel(before.task_id).await.unwrap();
        let mut entered = false;
        if super::start_work(&executor, before.task_id).await.unwrap() {
            async {
                entered = true;
            }
            .await;
        }
        assert!(!entered);
        assert_eq!(
            executor.get(before.task_id).await.unwrap().unwrap().status,
            veoveo_task_runtime::TaskStatus::Cancelled
        );

        // Timeseries gives committed cancellation priority even over a failed outcome.
        let before = running(&executor).await;
        remote.cancel(before.task_id).await.unwrap();
        let cancelled = settlement::settle(
            &executor,
            &before,
            TaskTransition::Failed(veoveo_task_runtime::TaskFailure::new(
                "forecast_failed",
                "native failure",
            )),
        )
        .await
        .unwrap();
        assert_eq!(cancelled.status, veoveo_task_runtime::TaskStatus::Cancelled);
        assert!(cancelled.result.is_none());
        assert!(cancelled.error.is_none());
        assert_eq!(cancelled.request, before.request);
        assert_eq!(cancelled.owner, before.owner);

        // A completion committed first cannot be rewritten by stale final settlement.
        let before = running(&executor).await;
        let completed = settlement::settle(&executor, &before, success())
            .await
            .unwrap();
        assert_eq!(remote.cancel(before.task_id).await.unwrap(), completed);
        assert_eq!(
            settlement::settle(&executor, &before, success())
                .await
                .unwrap(),
            completed
        );
        assert_eq!(
            completed.result,
            Some(serde_json::json!({"nativeSettlement":true}))
        );

        // An unrelated progress update is a genuine conflict, not cancellation proof.
        let before = running(&executor).await;
        executor
            .transition(
                before.task_id,
                TaskTransition::Running {
                    message: "forecast progress".into(),
                    progress: 0.5,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            settlement::settle(&executor, &before, success()).await,
            Err(veoveo_task_runtime::TaskError::Conflict(_))
        ));
        assert_eq!(
            executor.payload_state(before.task_id).await.unwrap(),
            TaskPayloadState::Running
        );

        // Only the executing worker may settle cancellation under its live lease.
        let before = running(&executor).await;
        remote.cancel(before.task_id).await.unwrap();
        assert!(matches!(
            settlement::update(&remote, before.task_id, success()).await,
            Err(veoveo_task_runtime::TaskError::LeaseHeld(_))
        ));
        assert_eq!(
            remote.get(before.task_id).await.unwrap().unwrap().status,
            veoveo_task_runtime::TaskStatus::CancelRequested
        );
        settlement::update(&executor, before.task_id, success())
            .await
            .unwrap();
        assert_eq!(
            remote.payload_state(before.task_id).await.unwrap(),
            TaskPayloadState::Cancelled
        );
    })
    .await
    .expect("Timeseries settlement controls exceeded 60 seconds");
}
