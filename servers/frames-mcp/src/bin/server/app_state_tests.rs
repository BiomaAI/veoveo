//! Actual Store initial worker admission; no transform or Artifact operation is dispatched.
use super::*;
use crate::tool_input_tests::fixture;
use std::{collections::BTreeSet, time::Duration};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskStatus};

fn owner() -> TaskOwner {
    use veoveo_types::{
        AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId,
        TenantId, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
    };
    let principal = PrincipalId::parse("operator").unwrap();
    TaskOwner {
        principal_key: "operator".into(),
        principal_kind: veoveo_task_runtime::PrincipalKind::Service,
        issuer: "https://frames.test".into(),
        subject: "operator".into(),
        profile: "operator".into(),
        tenant_key: Some("tenant-a".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::parse("frames-test").unwrap(),
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

#[tokio::test]
async fn remote_cancellation_before_frames_dispatch_is_inert() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "frames", "executor");
        let remote = TaskRuntime::new(db.b.clone(), "frames", "canceller");
        let owner = owner();
        let id = TaskId::new();
        runtime
            .create(CreateTask {
                task_id: id,
                owner,
                server: "frames".into(),
                task_type: const { veoveo_types::TaskTypeName::from_static("batch_transform") },
                request: serde_json::to_value(crate::BatchTaskRequest {
                    args: veoveo_frames_mcp::contract::BatchTransformRequest {
                        convert: veoveo_frames_mcp::contract::ConvertFrameRequest {
                            target: veoveo_frames_mcp::contract::CoordinateSpace::EcefWgs84,
                            points: vec![veoveo_frames_mcp::contract::CoordinatePoint::Wgs84(
                                veoveo_frames_mcp::contract::Wgs84Position {
                                    latitude_degrees: 0.0,
                                    longitude_degrees: 0.0,
                                    ellipsoid_height_m: 0.0,
                                },
                            )],
                            allow_approximation: false,
                        },
                        artifact: false,
                    },
                    operation_id: veoveo_frames_mcp::contract::CoordinateOperationId::parse(
                        format!("op-{}", uuid::Uuid::now_v7()),
                    )
                    .unwrap(),
                    operation_created_at: chrono::Utc::now(),
                    artifact_write_capability: None,
                })
                .unwrap(),
                recovery_class: RecoveryClass::Resume,
                idempotency_key: None,
                ttl_ms: None,
                poll_interval_ms: None,
                retention_pins: BTreeSet::new(),
            })
            .await
            .unwrap();
        runtime.claim(id, Duration::from_secs(30)).await.unwrap();
        remote.cancel(id).await.unwrap();
        let mut entered = false;
        if start_work(&runtime, id).await.unwrap() {
            async {
                entered = true;
            }
            .await;
        }
        assert!(!entered);
        assert_eq!(
            runtime.get(id).await.unwrap().unwrap().status,
            TaskStatus::Cancelled
        );
    })
    .await
    .expect("Frames initial Task checkpoint exceeded 60 seconds");
}
