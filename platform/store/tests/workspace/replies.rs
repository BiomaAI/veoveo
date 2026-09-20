use super::runs::{admission, agent_id, run_id, update};
use super::*;
use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use veoveo_platform_store::workspace::{
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
        let db = TestDb::new().await;
        let alice = identity(&db.a, "alice").await;
        let bob = identity(&db.a, "bob").await;
        context(&db.a, &alice, "research").await;
        let a = authority(&db.a, &alice, "research").await;
        let b = authority(&db.b, &bob, "research").await;
        let chat = WorkspaceChatId::new();
        let other = WorkspaceChatId::new();
        db.a.create_workspace_chat(&a, chat, "Replies")
            .await
            .unwrap();
        db.a.create_workspace_chat(&a, other, "Other")
            .await
            .unwrap();
        join(&db.a, &a, &b, &bob, chat).await;
        let agent =
            db.a.add_workspace_agent(&a, chat, uuid::Uuid::now_v7(), admission("Reviewer"))
                .await
                .unwrap();
        let original = WorkspaceMessageId::new();
        let mut first = message(original, None);
        first.text = "界".repeat(550);
        first.addressed_agents = vec![agent_id(&agent.id)];
        let turn = db.a.send_workspace_turn(&a, chat, first).await.unwrap();
        let run = &turn.runs[0];
        let response = Target::Response(run_id(run));
        let head = db.a.workspace_head(&a, chat).await.unwrap();
        assert_eq!(
            db.a.send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), Some(response)))
                .await,
            Err(WorkspaceError::Conflict)
        );
        assert_eq!(
            db.a.workspace_head(&a, chat).await.unwrap(),
            head,
            "running-response rejection commits no message"
        );
        db.a.client()
            .query("UPDATE ONLY $person SET display_name = $name")
            .bind(("person", alice.principal_id.record_id()))
            .bind(("name", "界".repeat(200)))
            .await
            .unwrap()
            .check()
            .unwrap();
        let human =
            db.a.send_workspace_turn(
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
        db.a.claim_workspace_run(&a, chat, run_id(run), fence)
            .await
            .unwrap();
        db.a.update_workspace_run(
            &a,
            chat,
            run_id(run),
            update(fence, "Shared review", WorkspaceRunState::Completed),
        )
        .await
        .unwrap();
        let id = WorkspaceMessageId::new();
        let (first, repeated) = tokio::join!(
            db.a.send_workspace_turn(&a, chat, message(id, Some(response))),
            db.b.send_workspace_turn(&a, chat, message(id, Some(response))),
        );
        let first = first.unwrap();
        assert_eq!(first, repeated.unwrap());
        assert_eq!(first.message.reply_to, Some(run.id.clone()));
        let quote = first.message.reply_context.as_ref().unwrap();
        assert_eq!(quote.author_name, "Reviewer");
        assert_eq!(quote.text, "Shared review");
        assert_eq!(
            db.a.send_workspace_turn(&a, chat, message(id, Some(Target::Message(original))))
                .await,
            Err(WorkspaceError::Conflict)
        );
        for target in [Target::Message(original), response] {
            assert_eq!(
                db.a.send_workspace_turn(
                    &a,
                    other,
                    message(WorkspaceMessageId::new(), Some(target))
                )
                .await,
                Err(WorkspaceError::NotFound)
            );
        }
        let missing = Target::Response(veoveo_platform_store::WorkspaceRunId::new());
        assert_eq!(
            db.a.send_workspace_turn(&a, chat, message(WorkspaceMessageId::new(), Some(missing)))
                .await,
            Err(WorkspaceError::NotFound)
        );
        db.a.remove_workspace_agent(&a, chat, agent_id(&agent.id))
            .await
            .unwrap();
        let restored =
            db.b.workspace_recent_snapshot(&b, chat, None, 1)
                .await
                .unwrap();
        assert_eq!(
            restored.messages.as_slice(),
            std::slice::from_ref(&first.message),
            "the quote survives a paged snapshot without its original response"
        );
        assert_eq!(
            db.b.send_workspace_turn(&a, chat, message(id, Some(response)))
                .await
                .unwrap(),
            first,
            "retry preserves the original quote after membership changes"
        );
        db.a.remove_workspace_member(&a, chat, bob.principal_id)
            .await
            .unwrap();
        assert_eq!(
            db.b.send_workspace_turn(&b, chat, message(WorkspaceMessageId::new(), Some(response)))
                .await,
            Err(WorkspaceError::NotFound)
        );
    })
    .await
    .unwrap();
}
