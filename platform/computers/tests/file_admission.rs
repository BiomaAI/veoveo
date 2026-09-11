#[path = "support/commands.rs"]
mod command_support;
mod support;
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputerError, ComputersStore,
    api::*,
    files::FileCapabilityRequest,
    secrets::{FileTransferAccess, FileTransferPayload},
};
use veoveo_mcp_contract::{
    ArtifactReadCapabilityId, ArtifactReadCapabilitySecret, ArtifactTaskId,
    IssuedArtifactReadCapability, LocalToolName,
};
use veoveo_task_runtime::{RecoveryClass, TaskRuntime};

async fn setup(
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
fn payload(path: &str) -> FileTransferPayload {
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

#[tokio::test]
async fn replicas_reserve_one_file_slot_and_recover_the_same_private_task() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = setup(&db).await;
    let request = Uuid::now_v7();
    let keys = command_support::keys();
    let input = payload("private-file-transfer-path");
    let attempts = futures::future::join_all((0..4).map(|i| {
        let (a, b, owner, input, keys) = (&a, &b, &owner, &input, &keys);
        async move {
            let store = if i % 2 == 0 { a } else { b };
            let authority = store
                .file_transfer_authority(owner, computer, None)
                .await
                .unwrap();
            store
                .queue_file_transfer(owner, authority, request, input, keys)
                .await
        }
    }))
    .await;
    let id = attempts[0].as_ref().unwrap().transfer_id();
    for result in attempts {
        let operation = result.unwrap();
        assert_eq!(operation.transfer_id(), id);
        a.ensure_file_task(&operation).await.unwrap();
        b.ensure_file_task(&operation).await.unwrap();
    }
    let task = TaskRuntime::new(db.b.clone(), "computers", "observer")
        .get(&id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.owner, *owner.owner());
    assert_eq!(task.task_type, "computer.file_transfer");
    assert_eq!(task.recovery_class, RecoveryClass::ProviderWait);
    assert_eq!(
        task.request,
        serde_json::json!({"computerId":computer,"transferId":id})
    );
    let found = b.pending_file_transfers(None, 10).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].transfer_id(), id);
    assert_eq!(
        keys.open_file_transfer(found[0].binding(), &{
            let mut row =
                db.a.client()
                    .query("SELECT VALUE sealed FROM ONLY $record;")
                    .bind((
                        "record",
                        surrealdb::types::RecordId::new(
                            "computer_file_transfer",
                            surrealdb::types::Uuid::from(id),
                        ),
                    ))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            let sealed: Option<veoveo_platform_store::OpenObject> = row.take(0).unwrap();
            serde_json::from_value(serde_json::to_value(sealed.unwrap()).unwrap()).unwrap()
        })
        .unwrap()
        .transfer()
        .path()
        .as_str(),
        "private-file-transfer-path"
    );
    let authority = b
        .file_transfer_authority(&owner, computer, None)
        .await
        .unwrap();
    assert!(matches!(
        b.queue_file_transfer(&owner, authority, request, &payload("changed"), &keys)
            .await,
        Err(ComputerError::RequestConflict)
    ));
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let authority = command_support::permit(&b, &agent, computer, grant.grant_id).await;
    assert!(matches!(
        b.queue_command(
            &agent,
            authority,
            Uuid::now_v7(),
            &command_support::payload("competing", 30),
            &keys
        )
        .await,
        Err(ComputerError::OperationBusy)
    ));
    // An owner can interrupt accepted work. Start remains fenced until that work
    // records confirmed containment; the file slot does not disappear on Stop admission.
    assert!(matches!(
        b.queue_maintenance(
            &owner,
            computer,
            Uuid::now_v7(),
            &veoveo_computers::maintenance::MaintenanceTarget {
                template_id: "development-next".into(),
                template_fingerprint: "b".repeat(64),
            }
        )
        .await,
        Err(ComputerError::OperationBusy)
    ));
    let stop = b
        .queue_operation(
            support::authenticated(owner.owner()),
            computer,
            Uuid::now_v7(),
            Action::Stop,
        )
        .await
        .unwrap();
    assert_eq!(stop.computer_id, computer);
    assert!(matches!(
        b.queue_operation(
            support::authenticated(owner.owner()),
            computer,
            Uuid::now_v7(),
            Action::Start
        )
        .await,
        Err(ComputerError::OperationBusy)
    ));
    let mut response=db.a.client().query("SELECT * FROM computer_file_transfer; SELECT * FROM outbox_event WHERE event_type='computer.file_transfer_queued'; SELECT * FROM computer_execution_slot;").await.unwrap().check().unwrap();
    for i in 0..3 {
        let rows: Vec<veoveo_platform_store::OpenObject> = response.take(i).unwrap();
        assert_eq!(rows.len(), 1);
        let encoded = serde_json::to_string(&rows).unwrap();
        assert!(!encoded.contains("private-file-transfer-path"));
    }
}

#[tokio::test]
async fn delegated_file_admission_enforces_bounds_and_revocation_at_commit() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = setup(&db).await;
    assert!(
        b.file_transfer_authority(&agent, computer, None)
            .await
            .is_err()
    );
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    assert!(
        b.file_transfer_authority(&agent, Uuid::now_v7(), Some(grant.grant_id))
            .await
            .is_err()
    );
    let authority = b
        .file_transfer_authority(&agent, computer, Some(grant.grant_id))
        .await
        .unwrap();
    let large = FileTransferPayload::new(
        FileTransfer::Import {
            artifact_id: Uuid::now_v7(),
            path: RetainedFilePath::try_from("data.bin".to_owned()).unwrap(),
        },
        FileTransferLimits {
            maximum_seconds: 30,
            maximum_bytes: 1025,
            on_interruption: AutomationInterruption::StopComputer,
        },
    )
    .unwrap();
    assert!(matches!(
        b.queue_file_transfer(
            &agent,
            authority,
            Uuid::now_v7(),
            &large,
            &command_support::keys()
        )
        .await,
        Err(ComputerError::InvalidInput)
    ));
    let authority = b
        .file_transfer_authority(&agent, computer, Some(grant.grant_id))
        .await
        .unwrap();
    a.revoke_automation_grant(
        &owner,
        &RevokeAutomationGrantInput {
            computer_id: computer,
            grant_id: grant.grant_id,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        b.queue_file_transfer(
            &agent,
            authority,
            Uuid::now_v7(),
            &payload("file"),
            &command_support::keys()
        )
        .await,
        Err(ComputerError::Forbidden)
    ));
    assert!(a.pending_file_transfers(None, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn artifact_access_is_task_bound_private_and_first_adequate_receipt_wins() {
    let db = support::TestDb::new().await;
    let (a, b, owner, _, computer) = setup(&db).await;
    let keys = command_support::keys();
    let input = FileTransferPayload::new(
        FileTransfer::Import {
            artifact_id: Uuid::now_v7(),
            path: RetainedFilePath::try_from("private-import".to_owned()).unwrap(),
        },
        payload("file").limits(),
    )
    .unwrap();
    let authority = a
        .file_transfer_authority(&owner, computer, None)
        .await
        .unwrap();
    let operation = a
        .queue_file_transfer(&owner, authority, Uuid::now_v7(), &input, &keys)
        .await
        .unwrap();
    let request = match operation.file_capability_request(&keys).unwrap().unwrap() {
        FileCapabilityRequest::Import(request) => request,
        _ => panic!("read capability required"),
    };
    assert_eq!(request.task_id.as_uuid(), operation.transfer_id());
    assert_eq!(request.max_artifact_count.get(), 1);
    assert_eq!(request.max_total_bytes.get(), 1024);
    let access = |task: Uuid| FileTransferAccess::Import {
        capability: IssuedArtifactReadCapability {
            capability_id: ArtifactReadCapabilityId::new(),
            secret: ArtifactReadCapabilitySecret::new("private-file-capability-fixture-1234567890")
                .unwrap(),
            task_id: ArtifactTaskId::parse(task.to_string()).unwrap(),
            expires_at: chrono::Utc::now() + chrono::TimeDelta::minutes(10),
        },
    };
    assert!(matches!(
        a.attach_file_artifact_access(&operation, access(Uuid::now_v7()), &keys)
            .await,
        Err(ComputerError::InvalidInput)
    ));
    let (left, right) = futures::join!(
        a.attach_file_artifact_access(&operation, access(operation.transfer_id()), &keys),
        b.attach_file_artifact_access(&operation, access(operation.transfer_id()), &keys)
    );
    assert!(
        left.unwrap()
            .file_capability_request(&keys)
            .unwrap()
            .is_none()
    );
    assert!(
        right
            .unwrap()
            .file_capability_request(&keys)
            .unwrap()
            .is_none()
    );
    let mut response =
        db.a.client()
            .query("SELECT * FROM computer_file_transfer;")
            .await
            .unwrap()
            .check()
            .unwrap();
    let rows: Vec<veoveo_platform_store::OpenObject> = response.take(0).unwrap();
    let encoded = serde_json::to_string(&rows).unwrap();
    assert!(!encoded.contains("private-file-capability"));
    assert!(!encoded.contains("private-import"));
}

#[tokio::test]
async fn stale_policy_and_changed_process_cannot_reserve_file_work() {
    let db = support::TestDb::new().await;
    let (a, b, owner, _, computer) = setup(&db).await;
    let keys = command_support::keys();
    let authority = a
        .file_transfer_authority(&owner, computer, None)
        .await
        .unwrap();
    let mut control = support::automation::control();
    let name = LocalToolName::new("transfer_file").unwrap();
    control.servers[0].tools.push(name.clone());
    control.policies[0].rules[0].tools.insert(name);
    support::policy::install(&db.a, control).await;
    assert!(matches!(
        a.queue_file_transfer(&owner, authority, Uuid::now_v7(), &payload("file"), &keys)
            .await,
        Err(ComputerError::Forbidden)
    ));
    let authority = b
        .file_transfer_authority(&owner, computer, None)
        .await
        .unwrap();
    db.a.client()
        .query("UPDATE $computer SET process_id='different-run-fixture';")
        .bind(("computer", command_support::computer_record(computer)))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        b.queue_file_transfer(&owner, authority, Uuid::now_v7(), &payload("file"), &keys)
            .await,
        Err(ComputerError::StateConflict)
    ));
    assert!(a.pending_file_transfers(None, 10).await.unwrap().is_empty());
}
