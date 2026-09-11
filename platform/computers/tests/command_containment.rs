#[path = "support/commands.rs"]
mod command_support;
mod support;
use command_support::*;
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputerError, ComputersStore, ReachedPhase, ReachedState, UndispatchedOutcome,
    api::*, commands::*,
};
use veoveo_task_runtime::{ClaimedTask, TaskRuntime, TaskStatus};

async fn dispatched(
    db: &support::TestDb,
) -> (
    ComputersStore,
    ComputersStore,
    ComputerActor,
    ClaimedTask,
    Uuid,
) {
    let (a, b, owner, agent, computer) = support::automation::setup(db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let claim = queue_claim(db, &a, &agent, computer, grant.grant_id).await;
    let ticket = a.begin_command_dispatch(&claim, &keys()).await.unwrap();
    assert_eq!(
        ticket.limits().on_interruption,
        AutomationInterruption::StopComputer
    );
    drop(ticket); // Native command outcome is unknown to this restarted fixture worker.
    (a, b, owner, claim, grant.grant_id)
}
fn stopped(operation: &CommandOperation) -> ReachedState {
    let binding = operation.binding();
    ReachedState {
        provider_instance_id: binding.provider_instance_id,
        computer_id: binding.computer_id,
        replacement_instance_id: binding.replacement_instance_id,
        template_fingerprint: binding.template_fingerprint.clone(),
        resource_id: binding.resource_id.clone(),
        process_id: binding.process_id.clone(),
        phase: ReachedPhase::Stopped,
    }
}
async fn slots(db: &support::TestDb) -> usize {
    let mut read =
        db.a.client()
            .query("SELECT * FROM computer_execution_slot;")
            .await
            .unwrap()
            .check()
            .unwrap();
    let rows: Vec<surrealdb::types::Value> = read.take(0).unwrap();
    rows.len()
}
async fn ready_read(db: &support::TestDb) {
    db.a.client().query("UPDATE computer_execution SET next_containment_read=time::now()-1s WHERE containment_reads > 0;").await.unwrap().check().unwrap();
}
async fn read_ticket(store: &ComputersStore, claim: &ClaimedTask) -> CommandContainmentRead {
    match store.admit_command_containment_read(claim).await.unwrap() {
        ContainmentReadAdmission::Read(ticket) => ticket,
        _ => panic!("expected one charged read"),
    }
}

#[tokio::test]
async fn cancelled_command_is_settled_only_after_original_run_termination() {
    let db = support::TestDb::new().await;
    let (a, b, owner, claim, grant) = dispatched(&db).await;
    let operation = a.command_for_claim(&claim).await.unwrap();
    let computer = operation.computer_id();
    assert!(
        a.abort_queued_command(&claim, CommandRefusal::AuthorityDenied)
            .await
            .is_err()
    );
    assert!(
        a.begin_command_containment(&claim, CommandInterruption::Cancelled)
            .await
            .is_err()
    );
    assert!(
        a.begin_command_containment(&claim, CommandInterruption::Deadline)
            .await
            .is_err()
    );
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "command-worker");
    tasks
        .cancel(&claim.snapshot.task_id.to_string())
        .await
        .unwrap();
    a.revoke_automation_grant(
        &owner,
        &RevokeAutomationGrantInput {
            computer_id: computer,
            grant_id: grant,
        },
    )
    .await
    .unwrap();
    let containing = a
        .begin_command_containment(&claim, CommandInterruption::Cancelled)
        .await
        .unwrap();
    assert_eq!(containing.stage(), CommandStage::Containing);
    assert!(containing.outcome().is_none());
    let (left, right) = futures::join!(a.admit_command_stop(&claim), b.admit_command_stop(&claim));
    assert_eq!(
        usize::from(matches!(left, Ok(Some(_)))) + usize::from(matches!(right, Ok(Some(_)))),
        1
    );
    drop((left, right)); // Both the live Stop response and its receipt can be lost.
    assert_eq!(
        a.get(owner.owner(), computer).await.unwrap().phase,
        ComputerPhase::Stopping
    );
    assert_eq!(slots(&db).await, 1);
    tasks.release_observation(&claim).await.unwrap();
    let successor = TaskRuntime::new(db.b.clone(), "computers", "successor")
        .claim_observation(
            &claim.snapshot.task_id.to_string(),
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    let same = b
        .begin_command_containment(&successor, CommandInterruption::ExecutionUnknown)
        .await
        .unwrap();
    assert_eq!(same.containment_id(), containing.containment_id());
    assert!(b.admit_command_stop(&successor).await.unwrap().is_none());
    let ticket = read_ticket(&b, &successor).await;
    let observed = stopped(ticket.operation());
    let terminal = b
        .complete_command_containment_read(&successor, ticket, observed)
        .await
        .unwrap();
    assert_eq!(terminal.stage(), CommandStage::Cancelled);
    assert_eq!(
        terminal.outcome(),
        Some(CommandOutcome::Terminated(CommandInterruption::Cancelled))
    );
    assert_eq!(slots(&db).await, 0);
    assert_eq!(
        a.get(owner.owner(), computer).await.unwrap().phase,
        ComputerPhase::Stopped
    );
    // Domain termination commits before shared Task projection and pin acknowledgement.
    let task = tasks
        .get(&claim.snapshot.task_id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, TaskStatus::CancelRequested);
    assert!(!task.retention_pins.is_empty());
    a.queue_operation(owner, computer, Uuid::now_v7(), Action::Start)
        .await
        .unwrap();
}

#[tokio::test]
async fn an_owner_stop_can_abort_before_containment_gets_its_first_stop_ticket() {
    let db = support::TestDb::new().await;
    let (a, _, owner, claim, _) = dispatched(&db).await;
    let computer = a.command_for_claim(&claim).await.unwrap().computer_id();
    let stop = a
        .queue_operation(owner, computer, Uuid::now_v7(), Action::Stop)
        .await
        .unwrap();
    a.ensure_operation_task(&stop.actor, stop.operation_id)
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "owner-stop-worker");
    let stop_claim = tasks
        .claim_observation(
            &stop.task_id().to_string(),
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    a.begin_command_containment(&claim, CommandInterruption::ExecutionUnknown)
        .await
        .unwrap();
    assert!(a.admit_command_stop(&claim).await.unwrap().is_none());
    tasks.cancel(&stop.task_id().to_string()).await.unwrap();
    a.abort_undispatched(&stop_claim, UndispatchedOutcome::CancelledBeforeDispatch)
        .await
        .unwrap();
    let ticket = a.admit_command_stop(&claim).await.unwrap().unwrap();
    let reached = stopped(ticket.operation());
    let terminal = a
        .complete_command_stop(&claim, ticket, reached)
        .await
        .unwrap();
    assert_eq!(
        terminal.outcome(),
        Some(CommandOutcome::Terminated(
            CommandInterruption::ExecutionUnknown
        ))
    );
    assert_eq!(terminal.stage(), CommandStage::Failed);
    assert_eq!(slots(&db).await, 0);
}

#[tokio::test]
async fn containment_does_not_clear_an_independent_owner_stop_fence() {
    let db = support::TestDb::new().await;
    let (a, _, owner, claim, _) = dispatched(&db).await;
    let computer = a.command_for_claim(&claim).await.unwrap().computer_id();
    let stop = a
        .queue_operation(owner, computer, Uuid::now_v7(), Action::Stop)
        .await
        .unwrap();
    a.ensure_operation_task(&stop.actor, stop.operation_id)
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "owner-stop-worker");
    let stop_claim = tasks
        .claim_observation(
            &stop.task_id().to_string(),
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    let native_stop = a.begin_dispatch(&stop_claim).await.unwrap();
    a.begin_command_containment(&claim, CommandInterruption::ExecutionUnknown)
        .await
        .unwrap();
    assert!(a.admit_command_stop(&claim).await.unwrap().is_none());
    let read = read_ticket(&a, &claim).await;
    let observed = stopped(read.operation());
    a.complete_command_containment_read(
        &claim,
        read,
        stopped(&a.command_for_claim(&claim).await.unwrap()),
    )
    .await
    .unwrap();
    let before = a.get(&stop.actor, computer).await.unwrap();
    assert_eq!(before.active_operation, Some(stop.operation_id));
    assert_eq!(before.phase, ComputerPhase::Stopping);
    a.complete_dispatch(&stop_claim, native_stop, observed)
        .await
        .unwrap();
    let after = a.get(&stop.actor, computer).await.unwrap();
    assert!(after.active_operation.is_none());
    assert_eq!(after.phase, ComputerPhase::Stopped);
}

#[tokio::test]
async fn wrong_run_evidence_and_exhausted_reads_keep_the_slot_across_workers() {
    let db = support::TestDb::new().await;
    let (a, b, owner, claim, _) = dispatched(&db).await;
    let computer = a.command_for_claim(&claim).await.unwrap().computer_id();
    a.begin_command_containment(&claim, CommandInterruption::ExecutionUnknown)
        .await
        .unwrap();
    let stop = a.admit_command_stop(&claim).await.unwrap().unwrap();
    let mut wrong = stopped(stop.operation());
    wrong.process_id = "replacement-run".into();
    assert!(matches!(
        a.complete_command_stop(&claim, stop, wrong).await,
        Err(ComputerError::StateConflict)
    ));
    for _ in 0..8 {
        ready_read(&db).await;
        let ticket = read_ticket(&a, &claim).await;
        let mut wrong = stopped(ticket.operation());
        wrong.phase = ReachedPhase::Ready;
        assert!(
            a.complete_command_containment_read(&claim, ticket, wrong)
                .await
                .is_err()
        );
    }
    TaskRuntime::new(db.a.clone(), "computers", "command-worker")
        .release_observation(&claim)
        .await
        .unwrap();
    let claim = TaskRuntime::new(db.b.clone(), "computers", "successor")
        .claim_observation(
            &claim.snapshot.task_id.to_string(),
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert!(matches!(
        b.admit_command_containment_read(&claim).await.unwrap(),
        ContainmentReadAdmission::RecoveryRequired
    ));
    assert!(matches!(
        b.admit_command_containment_read(&claim).await.unwrap(),
        ContainmentReadAdmission::RecoveryRequired
    ));
    assert_eq!(slots(&db).await, 1);
    assert_eq!(
        b.command_for_claim(&claim).await.unwrap().stage(),
        CommandStage::RecoveryRequired
    );
    assert_eq!(
        a.get(owner.owner(), computer).await.unwrap().phase,
        ComputerPhase::RecoveryRequired
    );
    assert!(b.admit_command_stop(&claim).await.is_err());
    let mut events = db
        .a
        .client()
        .query(
            "SELECT * FROM outbox_event WHERE event_type = 'computer.execution_recovery_required';",
        )
        .await
        .unwrap()
        .check()
        .unwrap();
    let events: Vec<surrealdb::types::Value> = events.take(0).unwrap();
    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn queued_cancellation_releases_only_an_undispatched_slot() {
    let db = support::TestDb::new().await;
    let (a, _, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    assert!(
        a.abort_queued_command(&claim, CommandRefusal::CancelledBeforeDispatch)
            .await
            .is_err()
    );
    TaskRuntime::new(db.a.clone(), "computers", "client")
        .cancel(&claim.snapshot.task_id.to_string())
        .await
        .unwrap();
    let terminal = a
        .abort_queued_command(&claim, CommandRefusal::CancelledBeforeDispatch)
        .await
        .unwrap();
    assert_eq!(
        terminal.outcome(),
        Some(CommandOutcome::Undispatched(
            CommandRefusal::CancelledBeforeDispatch
        ))
    );
    assert_eq!(terminal.stage(), CommandStage::Cancelled);
    assert_eq!(slots(&db).await, 0);
    assert_eq!(
        a.get(owner.owner(), computer).await.unwrap().phase,
        ComputerPhase::Ready
    );
    assert!(a.begin_command_dispatch(&claim, &keys()).await.is_err());
    assert!(a.acknowledge_command_task(&terminal).await.is_err());
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "command-worker");
    tasks
        .transition(
            &terminal.task_id().to_string(),
            veoveo_task_runtime::TaskTransition::Cancelled,
        )
        .await
        .unwrap();
    a.acknowledge_command_task(&terminal).await.unwrap();
    assert_eq!(a.pending_commands(None, 100).await.unwrap().len(), 1);
    // Another worker can repair a crash between the delivery marker and pin acknowledgement.
    a.acknowledge_command_task(&terminal).await.unwrap();
    tasks
        .acknowledge_retention_pin(
            &terminal.task_id().to_string(),
            &veoveo_task_runtime::TaskRetentionPin::new(format!(
                "computer-execution/{}",
                terminal.execution_id()
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(a.pending_commands(None, 100).await.unwrap().is_empty());
    assert!(a.ensure_command_task(&terminal).await.is_err());
}

#[tokio::test]
async fn an_expired_containment_deadline_cannot_gain_a_new_budget_after_restart() {
    let db = support::TestDb::new().await;
    let (a, b, _, claim, _) = dispatched(&db).await;
    a.begin_command_containment(&claim, CommandInterruption::ExecutionUnknown)
        .await
        .unwrap();
    // Backdate this isolated fixture's coherent event times instead of waiting three minutes.
    db.a.client().query("LET $now=time::now(); UPDATE computer_execution SET created_at=$now-300s, dispatched_at=$now-299s, execution_deadline=$now-270s, containment_started_at=$now-240s, containment_deadline=$now-60s;")
        .await.unwrap().check().unwrap();
    assert!(a.admit_command_stop(&claim).await.unwrap().is_none());
    assert!(matches!(
        b.admit_command_containment_read(&claim).await.unwrap(),
        ContainmentReadAdmission::RecoveryRequired
    ));
    assert_eq!(slots(&db).await, 1);
    let mut response =
        db.a.client()
            .query("SELECT VALUE containment_reads FROM computer_execution;")
            .await
            .unwrap()
            .check()
            .unwrap();
    let reads: Vec<u32> = response.take(0).unwrap();
    assert_eq!(reads, [0]);
}

#[tokio::test]
async fn containment_cannot_stop_or_settle_a_replacement_run() {
    let db = support::TestDb::new().await;
    let (a, _, owner, claim, _) = dispatched(&db).await;
    let command = a
        .begin_command_containment(&claim, CommandInterruption::ExecutionUnknown)
        .await
        .unwrap();
    db.a.client()
        .query("UPDATE $computer SET process_id='replacement-run';")
        .bind(("computer", computer_record(command.computer_id())))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(a.admit_command_stop(&claim).await.is_err());
    let read = read_ticket(&a, &claim).await;
    assert!(
        a.complete_command_containment_read(&claim, read, stopped(&command))
            .await
            .is_err()
    );
    let current = a.get(owner.owner(), command.computer_id()).await.unwrap();
    assert_eq!(current.process_id.as_deref(), Some("replacement-run"));
    assert_eq!(current.phase, ComputerPhase::Ready);
    assert_eq!(slots(&db).await, 1);
}
