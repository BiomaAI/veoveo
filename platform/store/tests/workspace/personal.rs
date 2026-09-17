use super::*;
use futures::StreamExt;
use std::time::Duration;
use veoveo_platform_store::{
    WorkspaceOperationId,
    workspace::{WorkspaceOperationIntent, WorkspaceOperationOutcome},
};

#[tokio::test]
async fn personal_projection_is_private_and_receives_cross_connection_changes() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    context(&db.a, &alice, "personal").await;
    let a = authority(&db.a, &alice, "personal").await;
    let b = authority(&db.b, &bob, "personal").await;
    let chat = WorkspaceChatId::new();
    db.a.create_workspace_chat(&a, chat, "Private work")
        .await
        .unwrap();
    let mut hints = db.b.workspace_personal_wakes().await.unwrap();
    let id = WorkspaceOperationId::new();
    let admitted =
        db.a.start_workspace_operation(
            &a,
            id,
            WorkspaceOperationIntent {
                chat,
                run: None,
                profile: "workspace".into(),
                app_uri: None,
                tool: "fixture".into(),
                arguments: r#"{"secret":"private-input"}"#.into(),
            },
        )
        .await
        .unwrap();
    let wake = tokio::time::timeout(Duration::from_secs(3), hints.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(wake.principal, alice.principal_id.record_id());
    db.a.settle_workspace_operation(
        id,
        admitted.operation.fence,
        WorkspaceOperationOutcome::Completed(
            r#"{"content":[{"type":"text","text":"private-result"}]}"#.into(),
        ),
    )
    .await
    .unwrap();
    let own =
        db.b.workspace_personal_state(&a, "workspace")
            .await
            .unwrap();
    assert_eq!(own.operations.len(), 1);
    assert!(!format!("{own:?}").contains("private-input"));
    assert!(!format!("{own:?}").contains("private-result"));
    assert!(
        db.b.workspace_personal_state(&b, "workspace")
            .await
            .unwrap()
            .operations
            .is_empty()
    );
    assert!(
        db.b.workspace_personal_state(&a, "another-profile")
            .await
            .unwrap()
            .operations
            .is_empty()
    );
    db.a.invite_workspace_member(&a, chat, WorkspaceInvitationId::new(), bob.principal_id)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while hints.next().await.unwrap().unwrap().principal != bob.principal_id.record_id() {}
    })
    .await
    .unwrap();
    let invited =
        db.b.workspace_personal_state(&b, "workspace")
            .await
            .unwrap();
    assert_eq!(invited.invitations.len(), 1);
    assert!(invited.operations.is_empty());
    assert!(
        db.b.workspace_personal_state(&a, "workspace")
            .await
            .unwrap()
            .invitations
            .is_empty()
    );
}
