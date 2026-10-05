use super::runs::{admission, agent_id, run_id};
use super::*;
use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use veoveo_workspace::persistence::WorkspaceRepository;
use veoveo_workspace::persistence::{
    WorkspaceAgentId, WorkspaceParticipation, WorkspaceParticipationMode as Mode,
    WorkspaceRunFailure as Failure, WorkspaceRunState as State, WorkspaceTurnRequest,
};

fn message(id: WorkspaceMessageId, agents: &[WorkspaceAgentId]) -> WorkspaceTurnRequest {
    WorkspaceTurnRequest {
        id,
        text: "Please discuss".into(),
        reply_to: None,
        attachments: vec![],
        addressed_agents: agents.to_vec(),
        deadline: Utc::now() + TimeDelta::seconds(120),
    }
}
async fn policy(
    store: &PlatformStore,
    actor: &WorkspaceAuthority,
    chat: WorkspaceChatId,
    mode: Mode,
    agents: &[WorkspaceAgentId],
) -> Result<veoveo_workspace::persistence::WorkspaceChat, WorkspaceError> {
    let current = WorkspaceRepository::new(store.clone())
        .workspace_snapshot(actor, chat, 0, 1)
        .await?
        .chat;
    WorkspaceRepository::new(store.clone())
        .update_workspace_settings(
            actor,
            chat,
            WorkspaceSettings {
                expected_revision: current.revision,
                title: current.title,
                archived: current.archived,
                members_can_invite: current.members_can_invite,
                owner: veoveo_platform_store::PrincipalId::from_uuid(runs::record_uuid(
                    &current.owner,
                )),
                participation: WorkspaceParticipation {
                    mode,
                    agents: agents.iter().map(|a| a.record_id()).collect(),
                },
            },
        )
        .await
}

#[tokio::test]
async fn owner_policy_resolves_once_and_replay_preserves_original_targets_and_context() {
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
    context(&db.a, &alice, "shared").await;
    let a = authority(&db.a, &alice, "shared").await;
    let chat = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, chat, "Responses")
        .await
        .unwrap();
    let writer = agent_id(
        &WorkspaceRepository::new(db.a.clone())
            .add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("writer"))
            .await
            .unwrap()
            .id,
    );
    let reviewer = agent_id(
        &WorkspaceRepository::new(db.a.clone())
            .add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("reviewer"))
            .await
            .unwrap()
            .id,
    );
    let plain = WorkspaceRepository::new(db.a.clone())
        .send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), &[]))
        .await
        .unwrap();
    assert!(plain.runs.is_empty());
    policy(&db.a, &a, chat, Mode::Default, &[writer])
        .await
        .unwrap();
    let id = WorkspaceMessageId::new();
    let first = WorkspaceRepository::new(db.a.clone())
        .send_workspace_turn(&a, chat, message(id, &[]))
        .await
        .unwrap();
    assert_eq!(first.runs.len(), 1);
    assert_eq!(first.runs[0].agent, writer.record_id());
    assert_eq!(first.runs[0].context_sequence, first.message.sequence);
    let explicit = WorkspaceRepository::new(db.a.clone())
        .send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), &[reviewer]))
        .await
        .unwrap();
    assert_eq!(explicit.runs.len(), 1);
    assert_eq!(explicit.runs[0].agent, reviewer.record_id());
    policy(&db.a, &a, chat, Mode::Automatic, &[writer, reviewer])
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .send_workspace_turn(&a, chat, message(id, &[]))
            .await
            .unwrap(),
        first
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(&a, chat, message(id, &[reviewer]))
            .await,
        Err(WorkspaceError::Conflict)
    );
    let automatic = WorkspaceRepository::new(db.a.clone())
        .send_workspace_turn(
            &a,
            chat,
            message(WorkspaceMessageId::new(), &[reviewer, reviewer]),
        )
        .await
        .unwrap();
    assert_eq!(automatic.runs.len(), 2);
    assert!(
        automatic
            .runs
            .iter()
            .all(|run| run.context_sequence == automatic.message.sequence)
    );
    assert_ne!(automatic.runs[0].id, automatic.runs[1].id);
    // Agent publication and read/recovery never resolve participation again.
    let run = &automatic.runs[0];
    let fence = Uuid::now_v7();
    WorkspaceRepository::new(db.a.clone())
        .claim_workspace_run(&a, chat, run_id(run), fence)
        .await
        .unwrap();
    WorkspaceRepository::new(db.a.clone())
        .update_workspace_run(
            &a,
            chat,
            run_id(run),
            runs::update(
                fence,
                "@reviewer this is output, not a trigger",
                State::Completed,
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .workspace_runs(&a, chat)
            .await
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .workspace_snapshot(&a, chat, 0, 100)
            .await
            .unwrap()
            .messages
            .len(),
        4
    );
}

#[tokio::test]
async fn concurrent_replay_is_atomic_and_busy_agents_leave_human_messages_writable() {
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
    context(&db.a, &alice, "shared").await;
    let a = authority(&db.a, &alice, "shared").await;
    let chat = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, chat, "Responses")
        .await
        .unwrap();
    let writer = agent_id(
        &WorkspaceRepository::new(db.a.clone())
            .add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("writer"))
            .await
            .unwrap()
            .id,
    );
    policy(&db.a, &a, chat, Mode::Default, &[writer])
        .await
        .unwrap();
    let id = WorkspaceMessageId::new();
    let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
    let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
    let (one, two) = tokio::join!(
        workspace_repository_a.send_workspace_turn(&a, chat, message(id, &[])),
        workspace_repository_b.send_workspace_turn(&a, chat, message(id, &[]))
    );
    let one = one.unwrap();
    assert_eq!(one, two.unwrap());
    for _ in 0..3 {
        WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), &[]))
            .await
            .unwrap();
    }
    let busy = WorkspaceRepository::new(db.b.clone())
        .send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), &[]))
        .await
        .unwrap();
    assert_eq!(busy.runs[0].state, State::Failed);
    assert_eq!(busy.runs[0].failure, Some(Failure::Capacity));
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_snapshot(&a, chat, 0, 100)
            .await
            .unwrap()
            .messages
            .len(),
        5
    );
    let fence = Uuid::now_v7();
    let running = WorkspaceRepository::new(db.a.clone())
        .claim_workspace_run(&a, chat, run_id(&one.runs[0]), fence)
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .reject_workspace_run(&a, chat, run_id(&one.runs[0]), Failure::Capacity)
            .await
            .unwrap(),
        running
    );
}

#[tokio::test]
async fn only_owner_controls_current_chat_agents_and_removal_clears_policy() {
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
    context(&db.a, &alice, "shared").await;
    let a = authority(&db.a, &alice, "shared").await;
    let b = authority(&db.b, &bob, "shared").await;
    let chat = WorkspaceChatId::new();
    let other = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, chat, "Responses")
        .await
        .unwrap();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, other, "Other")
        .await
        .unwrap();
    join(&db.a, &a, &b, &bob, chat).await;
    let writer = agent_id(
        &WorkspaceRepository::new(db.a.clone())
            .add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("writer"))
            .await
            .unwrap()
            .id,
    );
    let foreign = agent_id(
        &WorkspaceRepository::new(db.a.clone())
            .add_workspace_agent(&a, other, uuid::Uuid::now_v7(), admission("writer"))
            .await
            .unwrap()
            .id,
    );
    assert_eq!(
        policy(&db.b, &b, chat, Mode::Default, &[writer]).await,
        Err(WorkspaceError::Forbidden)
    );
    assert_eq!(
        policy(&db.a, &a, chat, Mode::Default, &[foreign]).await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), &[foreign]))
            .await,
        Err(WorkspaceError::NotFound)
    );
    assert!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_snapshot(&a, chat, 0, 100)
            .await
            .unwrap()
            .messages
            .is_empty()
    );
    let configured = policy(&db.a, &a, chat, Mode::Default, &[writer])
        .await
        .unwrap();
    WorkspaceRepository::new(db.a.clone())
        .remove_workspace_agent(&a, chat, writer)
        .await
        .unwrap();
    let removed = WorkspaceRepository::new(db.a.clone())
        .workspace_snapshot(&a, chat, 0, 100)
        .await
        .unwrap()
        .chat;
    assert_eq!(
        removed.participation,
        Some(WorkspaceParticipation::default())
    );
    assert_eq!(removed.revision, configured.revision + 1);
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), &[writer]))
            .await,
        Err(WorkspaceError::NotFound)
    );
    assert!(
        WorkspaceRepository::new(db.b.clone())
            .send_workspace_turn(&b, chat, message(WorkspaceMessageId::new(), &[]))
            .await
            .unwrap()
            .runs
            .is_empty()
    );
}
