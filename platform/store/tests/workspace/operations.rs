use super::*;
use std::time::Duration;
use uuid::Uuid;
use veoveo_platform_store::{
    WorkspaceOperationId,
    workspace::{
        WorkspaceOperationIntent, WorkspaceOperationOutcome as Outcome,
        WorkspaceOperationPhase as Phase,
    },
};

fn intent(chat: WorkspaceChatId) -> WorkspaceOperationIntent {
    WorkspaceOperationIntent {
        chat,
        run: None,
        profile: "workspace".into(),
        tool: "computers_create".into(),
        arguments: r#"{"name":"Research","template":"python"}"#.into(),
    }
}

#[tokio::test]
async fn operation_claims_are_private_durable_and_never_replay_unknown_dispatch() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let db = TestDb::new().await;
        let alice = identity(&db.a, "alice").await;
        let bob = identity(&db.a, "bob").await;
        context(&db.a, &alice, "operations").await;
        let a = authority(&db.a, &alice, "operations").await;
        let b = authority(&db.b, &bob, "operations").await;
        let chat = WorkspaceChatId::new();
        db.a.create_workspace_chat(&a, chat, "Private work in a shared chat")
            .await
            .unwrap();
        join(&db.a, &a, &b, &bob, chat).await;
        let id = WorkspaceOperationId::new();
        let (left, right) = tokio::join!(
            db.a.start_workspace_operation(&a, id, intent(chat)),
            db.b.start_workspace_operation(&a, id, intent(chat)),
        );
        let left = left.unwrap();
        let right = right.unwrap();
        assert_ne!(
            left.dispatch, right.dispatch,
            "only one caller may send tools/call"
        );
        assert_eq!(left.operation, right.operation);
        db.a.check_workspace_operation_dispatch(&a, id, left.operation.fence, None)
            .await
            .unwrap();
        assert_eq!(
            db.b.check_workspace_operation_dispatch(&a, id, Uuid::new_v4(), None)
                .await,
            Err(WorkspaceError::Conflict)
        );
        assert_eq!(
            db.b.workspace_operation(&b, id).await,
            Err(WorkspaceError::NotFound)
        );
        assert!(
            db.b.workspace_operations(&b, Some(chat), None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            db.b.start_workspace_operation(&b, id, intent(chat)).await,
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            db.a.start_workspace_operation(
                &a,
                id,
                WorkspaceOperationIntent {
                    arguments: "{}".into(),
                    ..intent(chat)
                }
            )
            .await,
            Err(WorkspaceError::Conflict)
        ));
        assert_eq!(
            db.b.settle_workspace_operation(id, Uuid::new_v4(), Outcome::Task("wrong".into()))
                .await,
            Err(WorkspaceError::Conflict)
        );

        // This is a timed-out client wait, not an assertion that the MCP action failed.
        db.a.client()
            .query("UPDATE $id SET dispatch_until = time::now() - 1s;")
            .bind(("id", id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            db.b.workspace_operation(&a, id).await.unwrap().phase,
            Phase::Unconfirmed
        );
        let repeated =
            db.b.start_workspace_operation(&a, id, intent(chat))
                .await
                .unwrap();
        assert!(!repeated.dispatch);
        assert_eq!(repeated.operation.phase, Phase::Unconfirmed);

        // A late definitive response remains recordable after the wait expired.
        let accepted =
            db.a.settle_workspace_operation(
                id,
                left.operation.fence,
                Outcome::Task("opaque/native/task?key=example".into()),
            )
            .await
            .unwrap();
        assert_eq!(accepted.phase, Phase::Task);
        assert_eq!(db.b.workspace_operation(&a, id).await.unwrap(), accepted);
        assert!(
            !db.b
                .start_workspace_operation(&a, id, intent(chat))
                .await
                .unwrap()
                .dispatch
        );
        assert_eq!(
            db.a.settle_workspace_operation(id, left.operation.fence, Outcome::Failed)
                .await,
            Err(WorkspaceError::Conflict)
        );

        // Leaving the room removes history, but does not orphan one's private Task receipt.
        let own = WorkspaceOperationId::new();
        let admitted =
            db.b.start_workspace_operation(&b, own, intent(chat))
                .await
                .unwrap();
        db.a.remove_workspace_member(&a, chat, bob.principal_id)
            .await
            .unwrap();
        assert_eq!(
            db.b.check_workspace_operation_dispatch(&b, own, admitted.operation.fence, None)
                .await,
            Err(WorkspaceError::NotFound)
        );
        db.a.settle_workspace_operation(
            own,
            admitted.operation.fence,
            Outcome::Task("bob-task".into()),
        )
        .await
        .unwrap();
        assert_eq!(
            db.b.workspace_snapshot(&b, chat, 0, 100).await,
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(
            db.b.workspace_operations(&b, None, None)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(matches!(
            db.b.start_workspace_operation(&b, WorkspaceOperationId::new(), intent(chat))
                .await,
            Err(WorkspaceError::NotFound)
        ));

        // Revocation prevents reads, while the original receipt writer can still
        // retain an already observed Task ID for later authorized recovery.
        let pending = WorkspaceOperationId::new();
        let admitted =
            db.a.start_workspace_operation(&a, pending, intent(chat))
                .await
                .unwrap();
        let older =
            db.b.workspace_operations(&a, None, Some(pending))
                .await
                .unwrap();
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].id, id.record_id());
        assert_eq!(
            db.b.workspace_operations(&b, None, Some(pending)).await,
            Err(WorkspaceError::NotFound)
        );
        db.a.client()
            .query("UPDATE $context SET policy_revision = 'changed';")
            .bind(("context", a.work_context.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        db.b.settle_workspace_operation(
            pending,
            admitted.operation.fence,
            Outcome::Task("retained-after-revocation".into()),
        )
        .await
        .unwrap();
        assert_eq!(
            db.a.workspace_operation(&a, pending).await,
            Err(WorkspaceError::Forbidden)
        );
    })
    .await
    .expect("bounded native operation receipt acceptance");
}

#[tokio::test]
async fn input_rounds_claim_one_revision_and_stale_forms_cannot_advance_them() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let db = TestDb::new().await;
        let alice = identity(&db.a, "alice").await;
        context(&db.a, &alice, "operations").await;
        let a = authority(&db.a, &alice, "operations").await;
        let chat = WorkspaceChatId::new();
        db.a.create_workspace_chat(&a, chat, "Input rounds")
            .await
            .unwrap();
        let id = WorkspaceOperationId::new();
        let started =
            db.a.start_workspace_operation(&a, id, intent(chat))
                .await
                .unwrap();
        let input =
            db.a.settle_workspace_operation(
                id,
                started.operation.fence,
                Outcome::InputRequired(r#"{"requestState":"opaque","inputRequests":{}}"#.into()),
            )
            .await
            .unwrap();
        let (left, right) = tokio::join!(
            db.a.resume_workspace_operation(&a, id, input.revision),
            db.b.resume_workspace_operation(&a, id, input.revision),
        );
        assert_ne!(left.is_ok(), right.is_ok());
        let resumed = left.or(right).unwrap();
        assert_eq!(resumed.phase, Phase::Dispatching);
        assert_eq!(resumed.round, 1);
        assert_ne!(resumed.fence, input.fence);
        assert_eq!(
            db.a.settle_workspace_operation(id, input.fence, Outcome::Failed)
                .await,
            Err(WorkspaceError::Conflict)
        );
        let next =
            db.a.settle_workspace_operation(
                id,
                resumed.fence,
                Outcome::InputRequired(
                    r#"{"requestState":"new opaque state","inputRequests":{}}"#.into(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(
            db.a.resume_workspace_operation(&a, id, input.revision)
                .await,
            Err(WorkspaceError::Conflict)
        );
        let resumed =
            db.b.resume_workspace_operation(&a, id, next.revision)
                .await
                .unwrap();
        let completed =
            db.b.settle_workspace_operation(
                id,
                resumed.fence,
                Outcome::Completed(r#"{"content":[],"isError":true}"#.into()),
            )
            .await
            .unwrap();
        assert_eq!(
            completed.phase,
            Phase::Completed,
            "a domain rejection is a completed MCP response"
        );
        assert!(
            !db.a
                .start_workspace_operation(&a, id, intent(chat))
                .await
                .unwrap()
                .dispatch
        );
    })
    .await
    .expect("bounded native MRTR fencing acceptance");
}
