#![allow(dead_code)] // Scenarios select independent parts of this shared fixture.
use crate::support;
#[path = "commands.rs"]
mod command_fixture;
pub use command_fixture::{computer_record, keys};
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputersStore,
    api::*,
    secrets::{FileTransferAccess, FileTransferPayload},
};
use veoveo_mcp_contract::LocalToolName;
use veoveo_task_runtime::{ClaimedTask, TaskRuntime};
pub async fn setup(
    db: &support::TestDb,
) -> (
    ComputersStore,
    ComputersStore,
    ComputerActor,
    ComputerActor,
    Uuid,
) {
    let state = support::automation::setup(db).await;
    let mut control = support::automation::control();
    for name in ["transfer_file", "update_template"] {
        let name = LocalToolName::new(name).unwrap();
        control.servers[0].tools.push(name.clone());
        control.policies[0].rules[0].tools.insert(name);
    }
    support::policy::install(&db.a, control).await;
    state
}
pub fn payload(path: &str) -> FileTransferPayload {
    FileTransferPayload::new(
        FileTransfer::Export {
            path: RetainedFilePath::try_from(path.to_owned()).unwrap(),
            filename: "output.bin".into(),
            media_type: "application/octet-stream".into(),
        },
        FileTransferLimits {
            maximum_seconds: 30,
            maximum_bytes: 1024,
            on_interruption: AutomationInterruption::StopComputer,
        },
    )
    .unwrap()
}

pub async fn queue_claim(
    db: &support::TestDb,
    store: &ComputersStore,
    actor: &ComputerActor,
    computer: Uuid,
    grant: Uuid,
) -> ClaimedTask {
    queue(
        db,
        store,
        actor,
        computer,
        Some(grant),
        &payload("private-transfer-file"),
    )
    .await
}
pub async fn queue(
    db: &support::TestDb,
    store: &ComputersStore,
    actor: &ComputerActor,
    computer: Uuid,
    grant: Option<Uuid>,
    payload: &FileTransferPayload,
) -> ClaimedTask {
    let authority = store
        .file_transfer_authority(actor, computer, grant)
        .await
        .unwrap();
    let operation = store
        .queue_file_transfer(actor, authority, Uuid::now_v7(), payload, &keys())
        .await
        .unwrap();
    let access = match payload.transfer() {
        FileTransfer::Export { .. } => FileTransferAccess::Export {
            capability: command_fixture::output_capability(operation.transfer_id()),
        },
        FileTransfer::Import { .. } => FileTransferAccess::Import {
            capability: veoveo_mcp_contract::IssuedArtifactReadCapability {
                capability_id: veoveo_mcp_contract::ArtifactReadCapabilityId::new(),
                secret: veoveo_mcp_contract::ArtifactReadCapabilitySecret::new(
                    "private-file-read-capability-fixture-1234567890",
                )
                .unwrap(),
                task_id: veoveo_mcp_contract::ArtifactTaskId::parse(
                    operation.transfer_id().to_string(),
                )
                .unwrap(),
                expires_at: chrono::Utc::now() + chrono::TimeDelta::minutes(10),
            },
        },
    };
    store
        .attach_file_artifact_access(&operation, access, &keys())
        .await
        .unwrap();
    store.ensure_file_task(&operation).await.unwrap();
    TaskRuntime::new(db.a.clone(), "computers", "file-worker")
        .claim_observation(
            &operation.task_id().to_string(),
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap()
}
