#![allow(dead_code)] // Shared setup is selected by each command scenario.
use crate::support;
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_computer_execution::ExecutionRequest;
use veoveo_computers::{
    ComputerActor, ComputersStore,
    api::*,
    automation_grants::AutomationAuthority,
    secrets::{CommandPayload, ComputerKeyRing, ComputerSealingKey},
};
use veoveo_platform_store::RecordId;
use veoveo_task_runtime::TaskRuntime;
use zeroize::Zeroizing;

pub fn keys() -> ComputerKeyRing {
    ComputerKeyRing::new(
        Uuid::from_u128(1),
        vec![ComputerSealingKey::new(Uuid::from_u128(1), Zeroizing::new([19; 32])).unwrap()],
    )
    .unwrap()
}
pub fn payload(value: &str, seconds: u32) -> CommandPayload {
    CommandPayload::new(
        ExecutionRequest::new(
            vec!["/bin/printf".into(), value.into()],
            ".".into(),
            BTreeMap::from([("TOKEN".into(), "private-command-environment-fixture".into())]),
            b"private-command-stdin-fixture".to_vec(),
        )
        .unwrap(),
        AutomationExecutionLimits {
            maximum_seconds: seconds,
            maximum_output_bytes: 1024,
            on_interruption: AutomationInterruption::StopComputer,
        },
    )
    .unwrap()
}
pub async fn permit(
    store: &ComputersStore,
    actor: &ComputerActor,
    computer: veoveo_computers_contract::ComputerId,
    grant: veoveo_computers_contract::AutomationGrantId,
) -> AutomationAuthority {
    store
        .authorize_automation_grant(actor, computer, grant, AutomationPermission::Execute)
        .await
        .unwrap()
}
pub fn computer_record(id: veoveo_computers_contract::ComputerId) -> RecordId {
    RecordId::new("computer", surrealdb::types::Uuid::from(id.as_uuid()))
}

pub async fn queue_claim(
    db: &support::TestDb,
    store: &ComputersStore,
    actor: &ComputerActor,
    computer: veoveo_computers_contract::ComputerId,
    grant: veoveo_computers_contract::AutomationGrantId,
) -> veoveo_task_runtime::ClaimedTask {
    let operation = store
        .queue_command(
            actor,
            permit(store, actor, computer, grant).await,
            veoveo_computers::api::RequestId::new(),
            &payload("private-dispatch-argument", 30),
            &keys(),
        )
        .await
        .unwrap();
    store
        .attach_command_output(&operation, output_capability(operation.task_id()), &keys())
        .await
        .unwrap();
    store.ensure_command_task(&operation).await.unwrap();
    TaskRuntime::new(db.a.clone(), "computers", "command-worker")
        .claim_observation(operation.task_id(), std::time::Duration::from_secs(60))
        .await
        .unwrap()
}

/// Synthetic issuance receipt for isolated domain tests. Native worker tests must
/// obtain a real capability from the Artifact service before publication.
pub fn output_capability(
    task: veoveo_types::TaskId,
) -> veoveo_mcp_contract::IssuedArtifactWriteCapability {
    veoveo_mcp_contract::IssuedArtifactWriteCapability {
        capability_id: veoveo_mcp_contract::ArtifactWriteCapabilityId::new(),
        secret: veoveo_mcp_contract::ArtifactWriteCapabilitySecret::new(
            "private-computer-output-capability-fixture",
        )
        .unwrap(),
        task_id: task.to_string(),
        expires_at: chrono::Utc::now() + chrono::TimeDelta::minutes(10),
    }
}

pub fn expected_task_output(
    operation: &veoveo_computers::commands::CommandOperation,
) -> Option<serde_json::Value> {
    match operation.outcome()? {
        veoveo_computers::commands::CommandOutcome::Completed(result) => Some(serde_json::json!({
            "content": [], "structuredContent": result, "isError": result.exit_code() != 0
        })),
        _ => None,
    }
}
