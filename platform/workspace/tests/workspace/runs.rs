use super::*;
use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use veoveo_workspace::persistence::WorkspaceRepository;
use veoveo_workspace::persistence::{
    WorkspaceAgentAdmission, WorkspaceAgentId, WorkspaceOperationId, WorkspaceOperationIntent,
    WorkspaceRun, WorkspaceRunFailure, WorkspaceRunId, WorkspaceRunState, WorkspaceRunUpdate,
};

static DIGEST: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| catalog_content().digest().unwrap());
pub(super) fn catalog_content() -> veoveo_agent_runtime::persistence::AgentContent {
    use veoveo_agent_runtime::persistence::*;
    AgentContent {
        model: AgentModelReference {
            id: "fixture-model".into(),
            revision: "a".repeat(64),
        },
        instructions: "Explicit store boundary fixture".into(),
        tools: vec![],
        budgets: AgentBudgets {
            max_output_tokens: 128,
            max_completion_calls: 4,
            max_tool_calls: 8,
            deadline_seconds: 120,
        },
        execution: AgentExecution::Chat,
    }
}

pub(super) fn admission(name: &str) -> WorkspaceAgentAdmission {
    WorkspaceAgentAdmission {
        definition: name.to_lowercase(),
        definition_digest: DIGEST.to_string(),
        display_name: name.into(),
        provider: "explicit-test-fixture".into(),
        model: "no-model-in-this-store-test".into(),
    }
}
pub(super) fn record_uuid(record: &surrealdb::types::RecordId) -> Uuid {
    match &record.key {
        surrealdb::types::RecordIdKey::Uuid(value) => **value,
        _ => panic!("expected fixture UUID"),
    }
}
pub(super) fn agent_id(run: &surrealdb::types::RecordId) -> WorkspaceAgentId {
    WorkspaceAgentId::from_uuid(record_uuid(run))
}
pub(super) fn run_id(run: &WorkspaceRun) -> WorkspaceRunId {
    WorkspaceRunId::from_uuid(record_uuid(&run.id))
}
pub(super) fn update(fence: Uuid, text: &str, state: WorkspaceRunState) -> WorkspaceRunUpdate {
    WorkspaceRunUpdate {
        fence,
        text: text.into(),
        state,
        feedback: Default::default(),
        failure: None,
    }
}

#[tokio::test]
async fn feedback_is_fenced_monotonic_and_wakes_without_text() {
    use veoveo_workspace::persistence::{WorkspaceRunFeedback, WorkspaceRunPhase};
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
        veoveo_workspace::schema::module_setup(
            fixture::module_lanes::execution("workspace").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "alice").await;
    context(&db.a, &alice, "feedback").await;
    let actor = authority(&db.a, &alice, "feedback").await;
    let chat = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&actor, chat, "Feedback")
        .await
        .unwrap();
    let agent = WorkspaceRepository::new(db.a.clone())
        .add_workspace_agent(&actor, chat, uuid::Uuid::now_v7(), admission("writer"))
        .await
        .unwrap();
    let trigger = WorkspaceMessageId::new();
    let deadline = Utc::now() + TimeDelta::seconds(120);
    WorkspaceRepository::new(db.a.clone())
        .send_workspace_turn(
            &actor,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: trigger,
                text: "Run".into(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline,
            },
        )
        .await
        .unwrap();
    let run = WorkspaceRepository::new(db.a.clone())
        .start_workspace_run(
            &actor,
            chat,
            agent_id(&agent.id),
            trigger,
            &DIGEST,
            deadline,
        )
        .await
        .unwrap();
    let fence = Uuid::now_v7();
    let claimed = WorkspaceRepository::new(db.a.clone())
        .claim_workspace_run(&actor, chat, run_id(&run), fence)
        .await
        .unwrap();
    let mut change = update(fence, "", WorkspaceRunState::Running);
    change.feedback = WorkspaceRunFeedback {
        phase: WorkspaceRunPhase::CallingTools,
        operations: 1,
    };
    let changed = WorkspaceRepository::new(db.a.clone())
        .update_workspace_run(&actor, chat, run_id(&run), change.clone())
        .await
        .unwrap();
    assert!(changed.updated_sequence > claimed.updated_sequence);
    let heartbeat = WorkspaceRepository::new(db.b.clone())
        .update_workspace_run(&actor, chat, run_id(&run), change.clone())
        .await
        .unwrap();
    assert_eq!(changed.updated_sequence, heartbeat.updated_sequence);
    change.feedback.operations = 0;
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .update_workspace_run(&actor, chat, run_id(&run), change.clone())
            .await,
        Err(WorkspaceError::Conflict)
    );
    change.feedback.operations = 1;
    WorkspaceRepository::new(db.b.clone())
        .cancel_workspace_run(&actor, chat, run_id(&run))
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .update_workspace_run(&actor, chat, run_id(&run), change)
            .await,
        Err(WorkspaceError::Conflict)
    );
}

#[tokio::test]
async fn two_agents_have_isolated_context_fenced_publication_and_independent_cancellation() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
        veoveo_workspace::schema::module_setup(
            fixture::module_lanes::execution("workspace").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    let eve = identity(&db.a, "eve").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let b = authority(&db.b, &bob, "research").await;
    let e = authority(&db.b, &eve, "research").await;
    let chat = WorkspaceChatId::new();
    let private = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, chat, "Shared")
        .await
        .unwrap();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, private, "Other chat")
        .await
        .unwrap();
    join(&db.a, &a, &b, &bob, chat).await;
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .add_workspace_agent(&b, chat, uuid::Uuid::now_v7(), admission("writer"))
            .await,
        Err(WorkspaceError::Forbidden)
    );
    let writer = WorkspaceRepository::new(db.a.clone())
        .add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("writer"))
        .await
        .unwrap();
    let reviewer = WorkspaceRepository::new(db.a.clone())
        .add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("reviewer"))
        .await
        .unwrap();
    let writer = agent_id(&writer.id);
    let reviewer = agent_id(&reviewer.id);
    let trigger = WorkspaceMessageId::new();
    WorkspaceRepository::new(db.a.clone())
        .send_workspace_turn(
            &a,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: trigger,
                text: ("Please discuss").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
    WorkspaceRepository::new(db.a.clone())
        .send_workspace_turn(
            &a,
            private,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: WorkspaceMessageId::new(),
                text: ("OTHER CHAT SECRET").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
    let deadline = Utc::now() + TimeDelta::seconds(120);
    let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
    let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
    let (one, two) = tokio::join!(
        workspace_repository_a.start_workspace_run(&a, chat, writer, trigger, &DIGEST, deadline),
        workspace_repository_b.start_workspace_run(&a, chat, reviewer, trigger, &DIGEST, deadline),
    );
    let one = one.unwrap();
    let two = two.unwrap();
    assert_ne!(one.id, two.id);
    assert_ne!(one.sequence, two.sequence);
    let repeated = WorkspaceRepository::new(db.b.clone())
        .start_workspace_run(&a, chat, writer, trigger, &DIGEST, deadline)
        .await
        .unwrap();
    assert_eq!(one, repeated);
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .start_workspace_run(&b, chat, writer, trigger, &DIGEST, deadline)
            .await,
        Err(WorkspaceError::Forbidden)
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .workspace_runs(&e, chat)
            .await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_run_context(&a, private, run_id(&one))
            .await,
        Err(WorkspaceError::NotFound)
    );
    let first_fence = Uuid::now_v7();
    let second_fence = Uuid::now_v7();
    let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
    let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
    let (claim1, claim2) = tokio::join!(
        workspace_repository_a.claim_workspace_run(&a, chat, run_id(&one), first_fence),
        workspace_repository_b.claim_workspace_run(&a, chat, run_id(&one), second_fence),
    );
    let fence = match (claim1, claim2) {
        (Ok(_), Err(WorkspaceError::Conflict)) => first_fence,
        (Err(WorkspaceError::Conflict), Ok(_)) => second_fence,
        other => panic!("exactly one worker must claim: {other:?}"),
    };
    WorkspaceRepository::new(db.b.clone())
        .claim_workspace_run(&a, chat, run_id(&two), second_fence)
        .await
        .unwrap();
    let operation_id = WorkspaceOperationId::new();
    let intent = |run_fence| WorkspaceOperationIntent {
        chat,
        run: Some((run_id(&one), run_fence)),
        profile: "workspace".into(),
        app_uri: None,
        tool: "fixture_task".into(),
        arguments: "{}".into(),
    };
    assert!(matches!(
        WorkspaceRepository::new(db.a.clone())
            .start_workspace_operation(&a, operation_id, intent(Uuid::new_v4()))
            .await,
        Err(WorkspaceError::Conflict)
    ));
    let operation = WorkspaceRepository::new(db.a.clone())
        .start_workspace_operation(&a, operation_id, intent(fence))
        .await
        .unwrap()
        .operation;
    WorkspaceRepository::new(db.b.clone())
        .check_workspace_operation_dispatch(&a, operation_id, operation.fence, Some(fence))
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .check_workspace_operation_dispatch(
                &a,
                operation_id,
                operation.fence,
                Some(Uuid::new_v4())
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    WorkspaceRepository::new(db.a.clone())
        .update_workspace_run(
            &a,
            chat,
            run_id(&one),
            update(fence, "First", WorkspaceRunState::Running),
        )
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .update_workspace_run(
                &a,
                chat,
                run_id(&one),
                update(Uuid::now_v7(), "First forged", WorkspaceRunState::Completed)
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    WorkspaceRepository::new(db.b.clone())
        .send_workspace_turn(
            &b,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: WorkspaceMessageId::new(),
                text: ("I can still contribute").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
    // Run attribution keeps disabled members but cannot disclose a foreign tenant name.
    db.a.client()
        .query(include_str!("../queries/workspace/people/configure.surql"))
        .bind(("principal", bob.principal_id.record_id()))
        .bind(("enabled", false))
        .bind(("kind", "user"))
        .await
        .unwrap()
        .check()
        .unwrap();
    let foreign =
        db.a.ensure_identity(
            "workspace-foreign",
            "foreign-run",
            "https://identity.test",
            "foreign-run",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    db.a.client()
        .query(include_str!(
            "../queries/workspace/people/foreign_member.surql"
        ))
        .bind((
            "member",
            surrealdb::types::RecordId::new(
                "workspace_member",
                surrealdb::types::Uuid::from(uuid::Uuid::new_v4()),
            ),
        ))
        .bind(("chat", chat.record_id()))
        .bind(("principal", foreign.principal_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let frozen = WorkspaceRepository::new(db.a.clone())
        .workspace_run_context(&a, chat, run_id(&one))
        .await
        .unwrap();
    assert!(
        frozen
            .people
            .iter()
            .any(|person| person.id == bob.principal_id.record_id())
    );
    assert!(
        !frozen
            .people
            .iter()
            .any(|person| person.id == foreign.principal_id.record_id())
    );
    // Re-enable Bob for the remaining authorization assertions in this scenario.
    db.a.client()
        .query(include_str!("../queries/workspace/people/configure.surql"))
        .bind(("principal", bob.principal_id.record_id()))
        .bind(("enabled", true))
        .bind(("kind", "user"))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(frozen.messages.len(), 1);
    assert_eq!(frozen.messages[0].text, "Please discuss");
    assert!(frozen.completed_runs.is_empty());
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .cancel_workspace_run(&b, chat, run_id(&one))
            .await,
        Err(WorkspaceError::Forbidden)
    );
    WorkspaceRepository::new(db.a.clone())
        .cancel_workspace_run(&a, chat, run_id(&one))
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .check_workspace_operation_dispatch(&a, operation_id, operation.fence, Some(fence))
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert!(matches!(
        WorkspaceRepository::new(db.a.clone())
            .start_workspace_operation(&a, WorkspaceOperationId::new(), intent(fence))
            .await,
        Err(WorkspaceError::Conflict)
    ));
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .update_workspace_run(
                &a,
                chat,
                run_id(&one),
                update(fence, "First late", WorkspaceRunState::Completed)
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    WorkspaceRepository::new(db.b.clone())
        .update_workspace_run(
            &a,
            chat,
            run_id(&two),
            update(
                second_fence,
                "Second complete",
                WorkspaceRunState::Completed,
            ),
        )
        .await
        .unwrap();
    let restored = WorkspaceRepository::new(db.b.clone())
        .workspace_runs(&b, chat)
        .await
        .unwrap();
    assert_eq!(restored.len(), 2);
    assert!(
        restored
            .iter()
            .any(|run| run.state == WorkspaceRunState::Cancelled && run.text == "First")
    );
    assert!(
        restored
            .iter()
            .any(|run| run.state == WorkspaceRunState::Completed && run.text == "Second complete")
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .start_workspace_run(&a, chat, writer, trigger, &DIGEST, deadline)
            .await
            .unwrap()
            .state,
        WorkspaceRunState::Cancelled
    );
}

#[tokio::test]
async fn lost_workers_and_revoked_members_cannot_publish_or_restart_on_replay() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
        veoveo_workspace::schema::module_setup(
            fixture::module_lanes::execution("workspace").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let b = authority(&db.b, &bob, "research").await;
    let chat = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, chat, "Recovery")
        .await
        .unwrap();
    join(&db.a, &a, &b, &bob, chat).await;
    let agent = WorkspaceRepository::new(db.a.clone())
        .add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("helper"))
        .await
        .unwrap();
    let agent = agent_id(&agent.id);
    let trigger = WorkspaceMessageId::new();
    WorkspaceRepository::new(db.b.clone())
        .send_workspace_turn(
            &b,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: trigger,
                text: ("Work").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
    let deadline = Utc::now() + TimeDelta::seconds(120);
    let run = WorkspaceRepository::new(db.b.clone())
        .start_workspace_run(&b, chat, agent, trigger, &DIGEST, deadline)
        .await
        .unwrap();
    let fence = Uuid::now_v7();
    WorkspaceRepository::new(db.b.clone())
        .claim_workspace_run(&b, chat, run_id(&run), fence)
        .await
        .unwrap();
    WorkspaceRepository::new(db.b.clone())
        .update_workspace_run(
            &b,
            chat,
            run_id(&run),
            update(fence, "Partial response", WorkspaceRunState::Running),
        )
        .await
        .unwrap();
    let before = WorkspaceRepository::new(db.a.clone())
        .workspace_head(&a, chat)
        .await
        .unwrap();
    // Deliberately expire only the owned fixture lease instead of sleeping.
    db.a.client()
        .query(include_str!("../queries/workspace/runs/lost_workers_and_revoked_members_cannot_publish_or_restart_on_replay/statement_1.surql"))
        .bind(("run", run.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
    let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
    let (first_watch, second_watch) = tokio::join!(
        workspace_repository_a.workspace_head(&a, chat),
        workspace_repository_b.workspace_head(&b, chat),
    );
    assert_eq!(
        first_watch.unwrap(),
        before + 1,
        "a chat watch must publish worker loss without an activity read"
    );
    assert_eq!(
        second_watch.unwrap(),
        before + 1,
        "concurrent watches settle the lost run only once"
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .update_workspace_run(
                &b,
                chat,
                run_id(&run),
                update(fence, "late", WorkspaceRunState::Completed)
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    let restored = WorkspaceRepository::new(db.a.clone())
        .workspace_runs(&a, chat)
        .await
        .unwrap();
    assert_eq!(restored[0].state, WorkspaceRunState::Interrupted);
    assert_eq!(restored[0].text, "Partial response");
    assert_eq!(restored[0].failure, Some(WorkspaceRunFailure::WorkerLost));
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .start_workspace_run(&b, chat, agent, trigger, &DIGEST, deadline)
            .await
            .unwrap()
            .state,
        WorkspaceRunState::Interrupted
    );
    let trigger2 = WorkspaceMessageId::new();
    WorkspaceRepository::new(db.b.clone())
        .send_workspace_turn(
            &b,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: trigger2,
                text: ("Explicit new work").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
    let run2 = WorkspaceRepository::new(db.b.clone())
        .start_workspace_run(&b, chat, agent, trigger2, &DIGEST, deadline)
        .await
        .unwrap();
    WorkspaceRepository::new(db.b.clone())
        .claim_workspace_run(&b, chat, run_id(&run2), fence)
        .await
        .unwrap();
    WorkspaceRepository::new(db.a.clone())
        .remove_workspace_member(&a, chat, bob.principal_id)
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .update_workspace_run(
                &b,
                chat,
                run_id(&run2),
                update(fence, "revoked", WorkspaceRunState::Completed)
            )
            .await,
        Err(WorkspaceError::NotFound)
    );
    let restored = WorkspaceRepository::new(db.a.clone())
        .workspace_runs(&a, chat)
        .await
        .unwrap();
    assert_eq!(
        restored[0].failure,
        Some(WorkspaceRunFailure::PermissionChanged)
    );
    assert!(restored[0].text.is_empty());
}

#[tokio::test]
async fn concurrent_admission_is_bounded_and_removing_an_agent_fences_all_its_runs() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
        veoveo_workspace::schema::module_setup(
            fixture::module_lanes::execution("workspace").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "alice").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let chat = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, chat, "Capacity")
        .await
        .unwrap();
    let agent = WorkspaceRepository::new(db.a.clone())
        .add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("helper"))
        .await
        .unwrap();
    let agent = agent_id(&agent.id);
    let mut triggers = Vec::new();
    for i in 0..5 {
        let trigger = WorkspaceMessageId::new();
        WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(
                &a,
                chat,
                veoveo_workspace::persistence::WorkspaceTurnRequest {
                    id: trigger,
                    text: format!("Work {i}"),
                    reply_to: None,
                    attachments: vec![],
                    addressed_agents: vec![],
                    deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
                },
            )
            .await
            .unwrap();
        triggers.push(trigger);
    }
    let deadline = Utc::now() + TimeDelta::seconds(120);
    let repository = WorkspaceRepository::new(db.a.clone());
    let runs = futures::future::join_all(triggers.iter().map(|trigger| {
        repository.start_workspace_run(&a, chat, agent, *trigger, &DIGEST, deadline)
    }))
    .await;
    assert_eq!(runs.iter().filter(|r| r.is_ok()).count(), 4);
    assert_eq!(
        runs.iter()
            .filter(|r| **r == Err(WorkspaceError::Conflict))
            .count(),
        1
    );
    let run = runs.into_iter().find_map(Result::ok).unwrap();
    let fence = Uuid::now_v7();
    WorkspaceRepository::new(db.a.clone())
        .claim_workspace_run(&a, chat, run_id(&run), fence)
        .await
        .unwrap();
    WorkspaceRepository::new(db.a.clone())
        .update_workspace_run(
            &a,
            chat,
            run_id(&run),
            update(fence, "Committed prefix", WorkspaceRunState::Running),
        )
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .update_workspace_run(
                &a,
                chat,
                run_id(&run),
                update(fence, "Out of order", WorkspaceRunState::Running)
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    WorkspaceRepository::new(db.a.clone())
        .remove_workspace_agent(&a, chat, agent)
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .update_workspace_run(
                &a,
                chat,
                run_id(&run),
                update(fence, "Committed prefix late", WorkspaceRunState::Completed)
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    let recovered = WorkspaceRepository::new(db.b.clone())
        .workspace_runs(&a, chat)
        .await
        .unwrap();
    assert_eq!(recovered.len(), 4);
    assert!(
        recovered
            .iter()
            .all(|run| run.state == WorkspaceRunState::Interrupted
                && run.failure == Some(WorkspaceRunFailure::PermissionChanged))
    );
}
