//! Inert capabilities and current Task products for native control-plane tests.
use super::task_results::RUN_COMPLETED;
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Duration};
use veoveo_stream_mcp::contract::RunId;
use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass, TaskOwner, TaskRuntime};
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TaskId, TenantId,
    WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};

pub(super) fn owner() -> TaskOwner {
    let principal = PrincipalId::parse("stream-result-test").unwrap();
    TaskOwner {
        principal_key: principal.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.example".into(),
        subject: "stream-result-test".into(),
        profile: "operator".into(),
        tenant_key: Some("stream-result-test".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::parse("stream-result-test").unwrap(),
            tenant: TenantId::parse("stream-result-test").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::parse("test-v1").unwrap(),
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

pub(super) fn current_output(id: TaskId) -> Value {
    let mut value: Value =
        serde_json::from_str(include_str!("../../../testdata/run-output.json")).unwrap();
    let id = RunId::try_from(id).unwrap();
    value["runUri"] = serde_json::to_value(veoveo_stream_mcp::contract::RunUri::new(id)).unwrap();
    value["resultUri"] =
        serde_json::to_value(veoveo_stream_mcp::contract::RunResultsUri::new(id)).unwrap();
    value["resultsArtifact"]["metadata"]["provenance"]["runId"] = id.to_string().into();
    value["annotationsArtifact"]["metadata"]["provenance"]["runId"] = id.to_string().into();
    value
}

pub(super) async fn create(runtime: &TaskRuntime, owner: TaskOwner, id: TaskId) {
    // These capabilities are inert fixture values. No Artifact or GPU service
    // participates in control-plane result-delivery qualification.
    let capability_id = "01983da0-0000-7000-8000-000000000004";
    let secret = "inert_fixture_capability_not_issued_0000";
    let expires_at = "2030-01-01T00:00:00Z".parse().unwrap();
    let task_id = veoveo_artifact_contract::ArtifactTaskId::parse(id.to_string()).unwrap();
    let request = super::tasks::DurableStreamRequest {
        input: serde_json::from_value::<super::tasks::StreamTaskInput>(json!({
            "operation": "run_recording",
            "video": {
                "recordingUri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
                "entityPath": "/camera/front", "timeline": "sensor_time",
                "range": {"start": 10, "end": 20}
            },
            "pipelineId": "traffic"
        }))
        .unwrap(),
        artifact_write_capability: veoveo_artifact_contract::IssuedArtifactWriteCapability {
            capability_id: veoveo_artifact_contract::ArtifactWriteCapabilityId::parse(
                capability_id,
            )
            .unwrap(),
            secret: veoveo_artifact_contract::ArtifactWriteCapabilitySecret::new(secret).unwrap(),
            task_id,
            expires_at,
        },
        artifact_read_capability: veoveo_artifact_contract::IssuedArtifactReadCapability {
            capability_id: veoveo_artifact_contract::ArtifactReadCapabilityId::parse(capability_id)
                .unwrap(),
            secret: veoveo_artifact_contract::ArtifactReadCapabilitySecret::new(secret).unwrap(),
            task_id,
            expires_at,
        },
    };
    let request = serde_json::to_value(request).unwrap();
    runtime
        .create(CreateTask {
            task_id: id,
            owner,
            server: "stream".into(),
            task_type: const { veoveo_types::TaskTypeName::from_static("run_recording") },
            request,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    runtime.claim(id, Duration::from_secs(30)).await.unwrap();
}

pub(super) async fn finish(runtime: &TaskRuntime, id: TaskId, stored: Value) {
    runtime
        .transition(
            id,
            veoveo_task_runtime::mcp_task_completion(
                RUN_COMPLETED,
                serde_json::from_value(stored).unwrap(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
}
pub(super) async fn corrupt_result(runtime: &TaskRuntime, id: TaskId, stored: Value) {
    let canonical =
        super::task_results::recording_result(serde_json::from_value(current_output(id)).unwrap())
            .unwrap();
    finish(runtime, id, serde_json::to_value(canonical).unwrap()).await;
    runtime
        .platform_store()
        .client()
        .query(include_str!(
            "../../../queries/bin/server/test_support/corrupt_result.surql"
        ))
        .bind(("task", veoveo_platform_store::task_record_id(id)))
        .bind((
            "result",
            veoveo_platform_store::TaskResultRecord::new(stored),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
}
