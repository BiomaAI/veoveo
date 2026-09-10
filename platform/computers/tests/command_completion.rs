mod support;
use support::commands::*;
use uuid::Uuid;
use veoveo_computers::{api::*, commands::*};
use veoveo_task_runtime::{TaskRetentionPin, TaskRuntime, TaskStatus, TaskTransition};

fn output(byte_count: u32) -> ExecutionOutput {
    // Domain fixture receipts only; native worker qualification must redeem real Artifacts.
    ExecutionOutput {
        artifact_id: Uuid::now_v7(),
        byte_count,
    }
}

#[tokio::test]
async fn known_exit_and_outputs_settle_once_before_task_projection_and_allow_the_next_command() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    for code in [0, 7] {
        let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
        let exit = a
            .begin_command_dispatch(&claim, &keys())
            .await
            .unwrap()
            .observe_exit(code, 32, 0)
            .unwrap();
        let stdout = output(32);
        let stderr = output(0);
        let completed = a
            .complete_command_output(&claim, exit, stdout, stderr)
            .await
            .unwrap();
        assert_eq!(completed.stage(), CommandStage::Completed);
        let Some(CommandOutcome::Completed(result)) = completed.outcome() else {
            panic!("missing known result");
        };
        assert_eq!(result.exit_code, code as u8);
        assert_eq!(result.stdout, stdout);
        assert_eq!(result.stderr, stderr);
        assert_eq!(result.computer_id, computer);
        assert_eq!(result.execution_id, completed.execution_id());
        assert_eq!(
            b.command_for_claim(&claim).await.unwrap().outcome(),
            completed.outcome()
        );
        assert!(b.begin_command_dispatch(&claim, &keys()).await.is_err());
        assert!(
            b.begin_command_containment(&claim, CommandInterruption::ExecutionUnknown)
                .await
                .is_err()
        );
        assert_eq!(
            a.get(owner.owner(), computer).await.unwrap().phase,
            ComputerPhase::Ready
        );
        assert!(a.acknowledge_command_task(&completed).await.is_err());
        let tasks = TaskRuntime::new(db.a.clone(), "computers", "command-worker");
        let task = tasks
            .get(&completed.task_id().to_string())
            .await
            .unwrap()
            .unwrap();
        assert!(!task.is_terminal());
        let pin = TaskRetentionPin::new(format!("computer-execution/{}", completed.execution_id()))
            .unwrap();
        assert!(task.retention_pins.contains(&pin));
        tasks.transition(&completed.task_id().to_string(), TaskTransition::Succeeded {
            message: "Command completed".into(),
            result: serde_json::json!({"content":[], "structuredContent":result, "isError":code != 0}),
        }).await.unwrap();
        // Corrupt only the disposable fixture projection. Status alone cannot
        // acknowledge a substituted output occurrence or wrong tool-error flag.
        db.a.client()
            .query("UPDATE $task SET result.isError = $wrong;")
            .bind(("task", completed.task_id().record_id()))
            .bind(("wrong", code == 0))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(b.acknowledge_command_task(&completed).await.is_err());
        assert!(!b.command_for_claim(&claim).await.unwrap().task_projected());
        db.a.client()
            .query("UPDATE $task SET result.isError = $correct;")
            .bind(("task", completed.task_id().record_id()))
            .bind(("correct", code != 0))
            .await
            .unwrap()
            .check()
            .unwrap();
        b.acknowledge_command_task(&completed).await.unwrap();
        let still_pending = b.pending_commands(None, 100).await.unwrap();
        assert!(
            still_pending
                .iter()
                .any(|c| c.execution_id() == completed.execution_id())
        );
        tasks
            .acknowledge_retention_pin(&completed.task_id().to_string(), &pin)
            .await
            .unwrap();
        assert!(b.pending_commands(None, 100).await.unwrap().is_empty());
        assert!(b.ensure_command_task(&completed).await.is_err());
    }
    let mut response = db.a.client().query("SELECT * FROM computer_execution_slot; SELECT * FROM outbox_event WHERE event_type='computer.execution_completed';").await.unwrap().check().unwrap();
    let slots: Vec<surrealdb::types::Value> = response.take(0).unwrap();
    let events: Vec<surrealdb::types::Value> = response.take(1).unwrap();
    assert!(slots.is_empty());
    assert_eq!(events.len(), 2);
    let encoded = serde_json::to_string(&events).unwrap();
    assert!(!encoded.contains("private-dispatch-argument"));
    assert!(!encoded.contains("private-computer-output-capability-fixture"));
}

#[tokio::test]
async fn invalid_outputs_lease_loss_replacement_and_containment_never_settle_a_known_result() {
    for scenario in [
        "bytes",
        "artifact",
        "lease",
        "run",
        "containment",
        "unknown_exit",
        "output_limit",
    ] {
        let db = support::TestDb::new().await;
        let (a, _, owner, agent, computer) = support::automation::setup(&db).await;
        let grant = a
            .issue_automation_grant(&owner, &support::automation::input(computer))
            .await
            .unwrap();
        let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
        let dispatch = a.begin_command_dispatch(&claim, &keys()).await.unwrap();
        let exit = dispatch.observe_exit(
            if scenario == "unknown_exit" { 124 } else { 0 },
            if scenario == "output_limit" { 1025 } else { 32 },
            0,
        );
        let mut stdout = output(32);
        if ["unknown_exit", "output_limit"].contains(&scenario) {
            assert!(exit.is_err());
        } else {
            match scenario {
                "bytes" => stdout.byte_count = 31,
                "artifact" => stdout.artifact_id = Uuid::nil(),
                "lease" => TaskRuntime::new(db.a.clone(), "computers", "command-worker")
                    .release_observation(&claim)
                    .await
                    .unwrap(),
                "run" => {
                    db.a.client()
                        .query("UPDATE $computer SET process_id='replacement-run';")
                        .bind(("computer", computer_record(computer)))
                        .await
                        .unwrap()
                        .check()
                        .unwrap();
                }
                "containment" => {
                    a.begin_command_containment(&claim, CommandInterruption::ExecutionUnknown)
                        .await
                        .unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                a.complete_command_output(&claim, exit.unwrap(), stdout, output(0))
                    .await
                    .is_err(),
                "{scenario}"
            );
        }
        let command = a.command_for_claim(&claim).await.unwrap();
        assert!(command.outcome().is_none(), "{scenario}");
        assert!(!command.is_terminal());
        let mut response = db.a.client().query("SELECT * FROM computer_execution_slot; SELECT * FROM outbox_event WHERE event_type='computer.execution_completed';").await.unwrap().check().unwrap();
        let slots: Vec<surrealdb::types::Value> = response.take(0).unwrap();
        let events: Vec<surrealdb::types::Value> = response.take(1).unwrap();
        assert_eq!(slots.len(), 1);
        assert!(events.is_empty());
    }
}

#[tokio::test]
async fn a_late_cancel_preserves_the_known_result_and_an_independent_owner_stop() {
    let db = support::TestDb::new().await;
    let (a, _, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    let exit = a
        .begin_command_dispatch(&claim, &keys())
        .await
        .unwrap()
        .observe_exit(0, 0, 0)
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "command-worker");
    tasks
        .cancel(&claim.snapshot.task_id.to_string())
        .await
        .unwrap();
    let stop = a
        .queue_operation(owner, computer, Uuid::now_v7(), Action::Stop)
        .await
        .unwrap();
    let command = a
        .complete_command_output(&claim, exit, output(0), output(0))
        .await
        .unwrap();
    assert_eq!(command.stage(), CommandStage::Completed);
    let task = tasks
        .get(&command.task_id().to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, TaskStatus::CancelRequested);
    let mut read =
        db.a.client()
            .query("SELECT VALUE active_operation FROM ONLY $computer;")
            .bind(("computer", computer_record(computer)))
            .await
            .unwrap()
            .check()
            .unwrap();
    let active: Option<Uuid> = read.take(0).unwrap();
    assert_eq!(active, Some(stop.operation_id));
}
