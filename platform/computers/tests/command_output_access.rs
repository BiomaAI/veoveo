mod support;
use support::commands::*;
use uuid::Uuid;
use veoveo_computers::{ComputerError, commands::CommandStage};
use veoveo_task_runtime::TaskRuntime;

#[tokio::test]
async fn output_capability_precedes_dispatch_survives_replica_loss_and_never_enters_task_or_audit()
{
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let keys = keys();
    let command = a
        .queue_command(
            &agent,
            permit(&a, &agent, computer, grant.grant_id).await,
            Uuid::now_v7(),
            &payload("private-command-fixture", 30),
            &keys,
        )
        .await
        .unwrap();
    a.ensure_command_task(&command).await.unwrap();
    let claim = TaskRuntime::new(db.a.clone(), "computers", "output-worker")
        .claim_observation(
            &command.task_id().to_string(),
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert!(matches!(
        a.begin_command_dispatch(&claim, &keys).await,
        Err(ComputerError::InvalidState)
    ));
    assert_eq!(
        a.command_for_claim(&claim).await.unwrap().stage(),
        CommandStage::Queued
    );
    let request = command.output_capability_request(&keys).unwrap().unwrap();
    assert_eq!(request.task_id, command.execution_id().to_string());
    assert_eq!(request.max_artifact_count.get(), 2);
    assert_eq!(request.max_total_bytes.get(), 1024);
    assert_eq!(
        request.required_data_labels,
        command.binding().required_output_labels
    );
    assert!(request.expires_at > chrono::Utc::now() + chrono::TimeDelta::seconds(440));
    assert!(
        a.attach_command_output(&command, output_capability(Uuid::now_v7()), &keys)
            .await
            .is_err()
    );
    let first = output_capability(command.execution_id());
    let second = output_capability(command.execution_id());
    let (left, right) = futures::join!(
        a.attach_command_output(&command, first.clone(), &keys),
        b.attach_command_output(&command, second.clone(), &keys)
    );
    assert!(left.is_ok() && right.is_ok());
    assert!(
        left.unwrap()
            .output_capability_request(&keys)
            .unwrap()
            .is_none()
    );
    assert!(
        right
            .unwrap()
            .output_capability_request(&keys)
            .unwrap()
            .is_none()
    );
    drop(a);
    let ticket = b.begin_command_dispatch(&claim, &keys).await.unwrap();
    assert!(
        [first.capability_id, second.capability_id]
            .contains(&ticket.output_access().capability().capability_id)
    );
    assert_eq!(ticket.output_access().maximum_output_bytes(), 1024);
    let winner = ticket.output_access().capability().capability_id;
    // A delayed losing issuance response does not change the capability after dispatch.
    b.attach_command_output(&command, output_capability(command.execution_id()), &keys)
        .await
        .unwrap();
    let mut response = db
        .a
        .client()
        .query("SELECT * FROM computer_execution; SELECT * FROM task; SELECT * FROM outbox_event;")
        .await
        .unwrap()
        .check()
        .unwrap();
    let executions: Vec<surrealdb::types::Value> = response.take(0).unwrap();
    let tasks: Vec<surrealdb::types::Value> = response.take(1).unwrap();
    let events: Vec<surrealdb::types::Value> = response.take(2).unwrap();
    for rows in [&executions, &tasks, &events] {
        let serialized = serde_json::to_string(rows).unwrap();
        assert!(!serialized.contains("private-computer-output-capability-fixture"));
        assert!(!serialized.contains("private-command-fixture"));
    }
    for rows in [&tasks, &events] {
        let serialized = serde_json::to_string(rows).unwrap();
        assert!(!serialized.contains("ciphertext"));
        assert!(!serialized.contains(&winner.to_string()));
    }
}

#[tokio::test]
async fn unavailable_or_short_lived_output_authority_never_dispatches_and_queued_renewal_is_fenced()
{
    let db = support::TestDb::new().await;
    let (a, _, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    let command = a.command_for_claim(&claim).await.unwrap();
    let keys = keys();
    let mut capability = output_capability(command.execution_id());
    capability.expires_at = chrono::Utc::now() + chrono::TimeDelta::seconds(140);
    assert!(matches!(
        a.attach_command_output(&command, capability.clone(), &keys)
            .await,
        Err(ComputerError::InvalidInput)
    ));
    // Coherent private fixture ages only the encrypted capability, with no wait or
    // provider call. A still-live receipt cannot cover 30s execution + 120s publish.
    let access =
        veoveo_computers::command_secrets::CommandOutputAccess::new(capability, 1024).unwrap();
    let sealed = keys.seal_output_access(command.binding(), &access).unwrap();
    let object: veoveo_platform_store::OpenObject =
        serde_json::from_value(serde_json::to_value(sealed).unwrap()).unwrap();
    db.a.client()
        .query("UPDATE computer_execution SET output_access=$sealed;")
        .bind(("sealed", object))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(a.begin_command_dispatch(&claim, &keys).await.is_err());
    assert_eq!(
        a.command_for_claim(&claim).await.unwrap().stage(),
        CommandStage::Queued
    );
    let renewed = output_capability(command.execution_id());
    a.attach_command_output(&command, renewed.clone(), &keys)
        .await
        .unwrap();
    // Cipher corruption is an operational fault, never permission to silently
    // replace an unreadable key/capability or execute without output controls.
    db.a.client()
        .query("UPDATE computer_execution SET output_access.ciphertext='corrupt';")
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(a.begin_command_dispatch(&claim, &keys).await.is_err());
    assert!(
        a.attach_command_output(&command, renewed, &keys)
            .await
            .is_err()
    );
    let mut result = db.a.client().query("SELECT * FROM outbox_event WHERE event_type='computer.execution_dispatched'; SELECT * FROM computer_execution_slot;").await.unwrap().check().unwrap();
    let events: Vec<surrealdb::types::Value> = result.take(0).unwrap();
    let slots: Vec<surrealdb::types::Value> = result.take(1).unwrap();
    assert!(events.is_empty());
    assert_eq!(slots.len(), 1);
}
