use super::*;
use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use veoveo_platform_store::{
    WorkspaceAgentId, WorkspaceOperationId, WorkspaceRunId,
    workspace::{
        WorkspaceAgentAdmission, WorkspaceOperationIntent, WorkspaceRun, WorkspaceRunFailure,
        WorkspaceRunState, WorkspaceRunUpdate,
    },
};

const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(super) fn admission(name: &str) -> WorkspaceAgentAdmission {
    WorkspaceAgentAdmission {
        definition: name.into(),
        definition_digest: "a".repeat(64),
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
        failure: None,
    }
}

#[tokio::test]
async fn two_agents_have_isolated_context_fenced_publication_and_independent_cancellation() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    let eve = identity(&db.a, "eve").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let b = authority(&db.b, &bob, "research").await;
    let e = authority(&db.b, &eve, "research").await;
    let chat = WorkspaceChatId::new();
    let private = WorkspaceChatId::new();
    db.a.create_workspace_chat(&a, chat, "Shared")
        .await
        .unwrap();
    db.a.create_workspace_chat(&a, private, "Other chat")
        .await
        .unwrap();
    join(&db.a, &a, &b, &bob, chat).await;
    assert_eq!(
        db.b.add_workspace_agent(&b, chat, admission("writer"))
            .await,
        Err(WorkspaceError::Forbidden)
    );
    let writer =
        db.a.add_workspace_agent(&a, chat, admission("writer"))
            .await
            .unwrap();
    let reviewer =
        db.a.add_workspace_agent(&a, chat, admission("reviewer"))
            .await
            .unwrap();
    let writer = agent_id(&writer.id);
    let reviewer = agent_id(&reviewer.id);
    let trigger = WorkspaceMessageId::new();
    db.a.send_workspace_turn(
        &a,
        chat,
        veoveo_platform_store::workspace::WorkspaceTurnRequest {
            id: trigger,
            text: ("Please discuss").to_owned(),
            reply_to: None,
            addressed_agents: vec![],
            deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
        },
    )
    .await
    .unwrap();
    db.a.send_workspace_turn(
        &a,
        private,
        veoveo_platform_store::workspace::WorkspaceTurnRequest {
            id: WorkspaceMessageId::new(),
            text: ("OTHER CHAT SECRET").to_owned(),
            reply_to: None,
            addressed_agents: vec![],
            deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
        },
    )
    .await
    .unwrap();
    let deadline = Utc::now() + TimeDelta::seconds(120);
    let (one, two) = tokio::join!(
        db.a.start_workspace_run(&a, chat, writer, trigger, DIGEST, deadline),
        db.b.start_workspace_run(&a, chat, reviewer, trigger, DIGEST, deadline),
    );
    let one = one.unwrap();
    let two = two.unwrap();
    assert_ne!(one.id, two.id);
    assert_ne!(one.sequence, two.sequence);
    let repeated =
        db.b.start_workspace_run(&a, chat, writer, trigger, DIGEST, deadline)
            .await
            .unwrap();
    assert_eq!(one, repeated);
    assert_eq!(
        db.b.start_workspace_run(&b, chat, writer, trigger, DIGEST, deadline)
            .await,
        Err(WorkspaceError::Forbidden)
    );
    assert_eq!(
        db.b.workspace_runs(&e, chat).await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        db.a.workspace_run_context(&a, private, run_id(&one)).await,
        Err(WorkspaceError::NotFound)
    );
    let first_fence = Uuid::now_v7();
    let second_fence = Uuid::now_v7();
    let (claim1, claim2) = tokio::join!(
        db.a.claim_workspace_run(&a, chat, run_id(&one), first_fence),
        db.b.claim_workspace_run(&a, chat, run_id(&one), second_fence),
    );
    let fence = match (claim1, claim2) {
        (Ok(_), Err(WorkspaceError::Conflict)) => first_fence,
        (Err(WorkspaceError::Conflict), Ok(_)) => second_fence,
        other => panic!("exactly one worker must claim: {other:?}"),
    };
    db.b.claim_workspace_run(&a, chat, run_id(&two), second_fence)
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
        db.a.start_workspace_operation(&a, operation_id, intent(Uuid::new_v4()))
            .await,
        Err(WorkspaceError::Conflict)
    ));
    let operation =
        db.a.start_workspace_operation(&a, operation_id, intent(fence))
            .await
            .unwrap()
            .operation;
    db.b.check_workspace_operation_dispatch(&a, operation_id, operation.fence, Some(fence))
        .await
        .unwrap();
    assert_eq!(
        db.b.check_workspace_operation_dispatch(
            &a,
            operation_id,
            operation.fence,
            Some(Uuid::new_v4())
        )
        .await,
        Err(WorkspaceError::Conflict)
    );
    db.a.update_workspace_run(
        &a,
        chat,
        run_id(&one),
        update(fence, "First", WorkspaceRunState::Running),
    )
    .await
    .unwrap();
    assert_eq!(
        db.b.update_workspace_run(
            &a,
            chat,
            run_id(&one),
            update(Uuid::now_v7(), "First forged", WorkspaceRunState::Completed)
        )
        .await,
        Err(WorkspaceError::Conflict)
    );
    db.b.send_workspace_turn(
        &b,
        chat,
        veoveo_platform_store::workspace::WorkspaceTurnRequest {
            id: WorkspaceMessageId::new(),
            text: ("I can still contribute").to_owned(),
            reply_to: None,
            addressed_agents: vec![],
            deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
        },
    )
    .await
    .unwrap();
    let frozen =
        db.a.workspace_run_context(&a, chat, run_id(&one))
            .await
            .unwrap();
    assert_eq!(frozen.messages.len(), 1);
    assert_eq!(frozen.messages[0].text, "Please discuss");
    assert!(frozen.completed_runs.is_empty());
    assert_eq!(
        db.b.cancel_workspace_run(&b, chat, run_id(&one)).await,
        Err(WorkspaceError::Forbidden)
    );
    db.a.cancel_workspace_run(&a, chat, run_id(&one))
        .await
        .unwrap();
    assert_eq!(
        db.b.check_workspace_operation_dispatch(&a, operation_id, operation.fence, Some(fence))
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert!(matches!(
        db.a.start_workspace_operation(&a, WorkspaceOperationId::new(), intent(fence))
            .await,
        Err(WorkspaceError::Conflict)
    ));
    assert_eq!(
        db.a.update_workspace_run(
            &a,
            chat,
            run_id(&one),
            update(fence, "First late", WorkspaceRunState::Completed)
        )
        .await,
        Err(WorkspaceError::Conflict)
    );
    db.b.update_workspace_run(
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
    let restored = db.b.workspace_runs(&b, chat).await.unwrap();
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
        db.a.start_workspace_run(&a, chat, writer, trigger, DIGEST, deadline)
            .await
            .unwrap()
            .state,
        WorkspaceRunState::Cancelled
    );
}

#[tokio::test]
async fn lost_workers_and_revoked_members_cannot_publish_or_restart_on_replay() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let b = authority(&db.b, &bob, "research").await;
    let chat = WorkspaceChatId::new();
    db.a.create_workspace_chat(&a, chat, "Recovery")
        .await
        .unwrap();
    join(&db.a, &a, &b, &bob, chat).await;
    let agent =
        db.a.add_workspace_agent(&a, chat, admission("helper"))
            .await
            .unwrap();
    let agent = agent_id(&agent.id);
    let trigger = WorkspaceMessageId::new();
    db.b.send_workspace_turn(
        &b,
        chat,
        veoveo_platform_store::workspace::WorkspaceTurnRequest {
            id: trigger,
            text: ("Work").to_owned(),
            reply_to: None,
            addressed_agents: vec![],
            deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
        },
    )
    .await
    .unwrap();
    let deadline = Utc::now() + TimeDelta::seconds(120);
    let run =
        db.b.start_workspace_run(&b, chat, agent, trigger, DIGEST, deadline)
            .await
            .unwrap();
    let fence = Uuid::now_v7();
    db.b.claim_workspace_run(&b, chat, run_id(&run), fence)
        .await
        .unwrap();
    // Deliberately expire only the owned fixture lease instead of sleeping.
    db.a.client()
        .query("UPDATE ONLY $run SET lease_until = time::now() - 1s;")
        .bind(("run", run.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        db.b.update_workspace_run(
            &b,
            chat,
            run_id(&run),
            update(fence, "late", WorkspaceRunState::Completed)
        )
        .await,
        Err(WorkspaceError::Conflict)
    );
    let restored = db.a.workspace_runs(&a, chat).await.unwrap();
    assert_eq!(restored[0].state, WorkspaceRunState::Interrupted);
    assert_eq!(restored[0].failure, Some(WorkspaceRunFailure::WorkerLost));
    assert_eq!(
        db.b.start_workspace_run(&b, chat, agent, trigger, DIGEST, deadline)
            .await
            .unwrap()
            .state,
        WorkspaceRunState::Interrupted
    );
    let trigger2 = WorkspaceMessageId::new();
    db.b.send_workspace_turn(
        &b,
        chat,
        veoveo_platform_store::workspace::WorkspaceTurnRequest {
            id: trigger2,
            text: ("Explicit new work").to_owned(),
            reply_to: None,
            addressed_agents: vec![],
            deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
        },
    )
    .await
    .unwrap();
    let run2 =
        db.b.start_workspace_run(&b, chat, agent, trigger2, DIGEST, deadline)
            .await
            .unwrap();
    db.b.claim_workspace_run(&b, chat, run_id(&run2), fence)
        .await
        .unwrap();
    db.a.remove_workspace_member(&a, chat, bob.principal_id)
        .await
        .unwrap();
    assert_eq!(
        db.b.update_workspace_run(
            &b,
            chat,
            run_id(&run2),
            update(fence, "revoked", WorkspaceRunState::Completed)
        )
        .await,
        Err(WorkspaceError::NotFound)
    );
    let restored = db.a.workspace_runs(&a, chat).await.unwrap();
    assert_eq!(
        restored[0].failure,
        Some(WorkspaceRunFailure::PermissionChanged)
    );
    assert!(restored[0].text.is_empty());
}

#[tokio::test]
async fn concurrent_admission_is_bounded_and_removing_an_agent_fences_all_its_runs() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "alice").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let chat = WorkspaceChatId::new();
    db.a.create_workspace_chat(&a, chat, "Capacity")
        .await
        .unwrap();
    let agent =
        db.a.add_workspace_agent(&a, chat, admission("helper"))
            .await
            .unwrap();
    let agent = agent_id(&agent.id);
    let mut triggers = Vec::new();
    for i in 0..5 {
        let trigger = WorkspaceMessageId::new();
        db.a.send_workspace_turn(
            &a,
            chat,
            veoveo_platform_store::workspace::WorkspaceTurnRequest {
                id: trigger,
                text: format!("Work {i}"),
                reply_to: None,
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
        triggers.push(trigger);
    }
    let deadline = Utc::now() + TimeDelta::seconds(120);
    let runs = futures::future::join_all(triggers.iter().map(|trigger| {
        db.a.start_workspace_run(&a, chat, agent, *trigger, DIGEST, deadline)
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
    db.a.claim_workspace_run(&a, chat, run_id(&run), fence)
        .await
        .unwrap();
    db.a.update_workspace_run(
        &a,
        chat,
        run_id(&run),
        update(fence, "Committed prefix", WorkspaceRunState::Running),
    )
    .await
    .unwrap();
    assert_eq!(
        db.a.update_workspace_run(
            &a,
            chat,
            run_id(&run),
            update(fence, "Out of order", WorkspaceRunState::Running)
        )
        .await,
        Err(WorkspaceError::Conflict)
    );
    db.a.remove_workspace_agent(&a, chat, agent).await.unwrap();
    assert_eq!(
        db.a.update_workspace_run(
            &a,
            chat,
            run_id(&run),
            update(fence, "Committed prefix late", WorkspaceRunState::Completed)
        )
        .await,
        Err(WorkspaceError::Conflict)
    );
    let recovered = db.b.workspace_runs(&a, chat).await.unwrap();
    assert_eq!(recovered.len(), 4);
    assert!(
        recovered
            .iter()
            .all(|run| run.state == WorkspaceRunState::Interrupted
                && run.failure == Some(WorkspaceRunFailure::PermissionChanged))
    );
}
