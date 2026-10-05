use super::runs::{admission, agent_id, run_id, update};
use super::*;
use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use veoveo_workspace::persistence::WorkspaceRepository;
use veoveo_workspace::persistence::{
    WorkspaceReplyTarget as Target, WorkspaceRunState, WorkspaceTurnRequest,
};

fn message(id: WorkspaceMessageId, reply_to: Option<Target>) -> WorkspaceTurnRequest {
    WorkspaceTurnRequest {
        id,
        text: "A reply".into(),
        reply_to,
        attachments: vec![],
        addressed_agents: vec![],
        deadline: Utc::now() + TimeDelta::seconds(120),
    }
}

#[tokio::test]
async fn replies_bind_same_chat_terminal_authors_and_keep_a_bounded_immutable_quote() {
    tokio::time::timeout(std::time::Duration::from_secs(40), async {
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
        let other = WorkspaceChatId::new();
        WorkspaceRepository::new(db.a.clone())
            .create_workspace_chat(&a, chat, "Replies")
            .await
            .unwrap();
        WorkspaceRepository::new(db.a.clone())
            .create_workspace_chat(&a, other, "Other")
            .await
            .unwrap();
        join(&db.a, &a, &b, &bob, chat).await;
        let agent = WorkspaceRepository::new(db.a.clone())
            .add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("Reviewer"))
            .await
            .unwrap();
        let original = WorkspaceMessageId::new();
        let mut first = message(original, None);
        first.text = "界".repeat(550);
        first.addressed_agents = vec![agent_id(&agent.id)];
        let turn = WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(&a, chat, first)
            .await
            .unwrap();
        let run = &turn.runs[0];
        let response = Target::Response(run_id(run));
        let head = WorkspaceRepository::new(db.a.clone())
            .workspace_head(&a, chat)
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), Some(response)))
                .await,
            Err(WorkspaceError::Conflict)
        );
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .workspace_head(&a, chat)
                .await
                .unwrap(),
            head,
            "running-response rejection commits no message"
        );
        db.a.client()
            .query(include_str!("../queries/workspace/replies/replies_bind_same_chat_terminal_authors_and_keep_a_bounded_immutable_quote/statement_1.surql"))
            .bind(("person", alice.principal_id.record_id()))
            .bind(("name", "界".repeat(200)))
            .await
            .unwrap()
            .check()
            .unwrap();
        let human = WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(
                &b,
                chat,
                message(WorkspaceMessageId::new(), Some(Target::Message(original))),
            )
            .await
            .unwrap();
        assert_eq!(
            human.message.reply_context.as_ref().unwrap().text,
            "界".repeat(500)
        );
        assert_eq!(
            human.message.reply_context.as_ref().unwrap().author_name,
            "界".repeat(160)
        );
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
                update(fence, "Shared review", WorkspaceRunState::Completed),
            )
            .await
            .unwrap();
        let id = WorkspaceMessageId::new();
        let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
        let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
        let (first, repeated) = tokio::join!(
            workspace_repository_a.send_workspace_turn(&a, chat, message(id, Some(response))),
            workspace_repository_b.send_workspace_turn(&a, chat, message(id, Some(response))),
        );
        let first = first.unwrap();
        assert_eq!(first, repeated.unwrap());
        assert_eq!(first.message.reply_to, Some(run.id.clone()));
        let quote = first.message.reply_context.as_ref().unwrap();
        assert_eq!(quote.author_name, "Reviewer");
        assert_eq!(quote.text, "Shared review");
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .send_workspace_turn(&a, chat, message(id, Some(Target::Message(original))))
                .await,
            Err(WorkspaceError::Conflict)
        );
        for target in [Target::Message(original), response] {
            assert_eq!(
                WorkspaceRepository::new(db.a.clone())
                    .send_workspace_turn(
                        &a,
                        other,
                        message(WorkspaceMessageId::new(), Some(target))
                    )
                    .await,
                Err(WorkspaceError::NotFound)
            );
        }
        let missing = Target::Response(veoveo_workspace::persistence::WorkspaceRunId::new());
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), Some(missing)))
                .await,
            Err(WorkspaceError::NotFound)
        );
        WorkspaceRepository::new(db.a.clone())
            .remove_workspace_agent(&a, chat, agent_id(&agent.id))
            .await
            .unwrap();
        let restored = WorkspaceRepository::new(db.b.clone())
            .workspace_recent_snapshot(&b, chat, None, 1)
            .await
            .unwrap();
        assert_eq!(
            restored.messages.as_slice(),
            std::slice::from_ref(&first.message),
            "the quote survives a paged snapshot without its original response"
        );
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .send_workspace_turn(&a, chat, message(id, Some(response)))
                .await
                .unwrap(),
            first,
            "retry preserves the original quote after membership changes"
        );
        WorkspaceRepository::new(db.a.clone())
            .remove_workspace_member(&a, chat, bob.principal_id)
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .send_workspace_turn(&b, chat, message(WorkspaceMessageId::new(), Some(response)))
                .await,
            Err(WorkspaceError::NotFound)
        );
    })
    .await
    .unwrap();
}
