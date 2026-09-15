//! Actual database acceptance for the shared conversation boundary. The fixture
//! owns a disposable store and never uses installation identities or data.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use fixture::TestDb;
use surrealdb::types::ToSql;
use veoveo_platform_store::{
    PlatformIdentity, PlatformStore, PrincipalKind, WorkContextId, WorkContextMembershipLevel,
    WorkspaceChatId, WorkspaceInvitationId, WorkspaceMessageId, deterministic_work_context_id,
    workspace::{WorkspaceAuthority, WorkspaceError, WorkspaceInvitationState, WorkspaceSettings},
};

async fn identity(store: &PlatformStore, key: &str) -> PlatformIdentity {
    store
        .ensure_identity(
            "workspace-test",
            key,
            "https://identity.test",
            key,
            PrincipalKind::User,
        )
        .await
        .unwrap()
}

async fn context(store: &PlatformStore, identity: &PlatformIdentity, key: &str) -> WorkContextId {
    let context = deterministic_work_context_id(&identity.tenant_key, key).unwrap();
    store.client().query(
        "CREATE ONLY $context SET tenant = $tenant, context_key = $key, title = $key,
         policy_revision = 'test-v1', memberships = [],
         output_policy = { owner_kind: 'principal', owner_key: 'alice', initial_grants: [], data_labels: [] };"
    ).bind(("context", context.record_id())).bind(("tenant", identity.tenant_id.record_id()))
        .bind(("key", key.to_owned())).await.unwrap().check().unwrap();
    context
}

async fn authority(
    store: &PlatformStore,
    identity: &PlatformIdentity,
    key: &str,
) -> WorkspaceAuthority {
    let version = store
        .artifact_read_context_version(&identity.tenant_key, key)
        .await
        .unwrap()
        .unwrap();
    WorkspaceAuthority::new(
        identity.tenant_id,
        deterministic_work_context_id(&identity.tenant_key, key).unwrap(),
        identity.principal_id,
        version.digest,
        WorkContextMembershipLevel::Contributor,
    )
}

async fn join(
    store: &PlatformStore,
    alice: &WorkspaceAuthority,
    bob: &WorkspaceAuthority,
    identity: &PlatformIdentity,
    chat: WorkspaceChatId,
) -> WorkspaceInvitationId {
    let invitation = WorkspaceInvitationId::new();
    store
        .invite_workspace_member(alice, chat, invitation, identity.principal_id)
        .await
        .unwrap();
    store
        .decide_workspace_invitation(bob, chat, invitation, WorkspaceInvitationState::Accepted)
        .await
        .unwrap();
    invitation
}

#[tokio::test]
async fn invitations_require_the_named_current_human_and_removal_revokes_history() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    let eve = identity(&db.a, "eve").await;
    let work_context = context(&db.a, &alice, "research").await;
    context(&db.a, &alice, "private").await;
    let a = authority(&db.a, &alice, "research").await;
    let b = authority(&db.b, &bob, "research").await;
    let e = authority(&db.b, &eve, "research").await;
    let private = authority(&db.a, &alice, "private").await;
    let chat = WorkspaceChatId::new();
    let created =
        db.a.create_workspace_chat(&a, chat, "Research")
            .await
            .unwrap();
    assert_eq!(created.sequence, 1);
    assert_eq!(
        db.b.create_workspace_chat(&a, chat, "Research")
            .await
            .unwrap(),
        created
    );
    assert_eq!(
        db.a.create_workspace_chat(&a, chat, "Changed").await,
        Err(WorkspaceError::Conflict)
    );
    for denied in [&b, &e, &private] {
        assert_eq!(
            db.b.workspace_snapshot(denied, chat, 0, 100).await,
            Err(WorkspaceError::NotFound)
        );
        assert!(db.b.list_workspace_chats(denied).await.unwrap().is_empty());
    }
    let original =
        db.a.send_workspace_message(&a, chat, WorkspaceMessageId::new(), "Shared history", None)
            .await
            .unwrap();
    let invitation = WorkspaceInvitationId::new();
    db.a.invite_workspace_member(&a, chat, invitation, bob.principal_id)
        .await
        .unwrap();
    let older_pending = WorkspaceInvitationId::new();
    db.a.invite_workspace_member(&a, chat, older_pending, bob.principal_id)
        .await
        .unwrap();
    assert_eq!(db.b.list_workspace_invitations(&b).await.unwrap().len(), 2);
    assert!(
        db.b.list_workspace_invitations(&e)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.b.decide_workspace_invitation(&e, chat, invitation, WorkspaceInvitationState::Accepted)
            .await,
        Err(WorkspaceError::NotFound)
    );
    db.b.decide_workspace_invitation(&b, chat, invitation, WorkspaceInvitationState::Accepted)
        .await
        .unwrap();
    let snapshot = db.b.workspace_snapshot(&b, chat, 0, 100).await.unwrap();
    assert_eq!(snapshot.messages, [original]);
    assert_eq!(snapshot.members.len(), 2);
    assert_eq!(
        db.b.invite_workspace_member(&b, chat, WorkspaceInvitationId::new(), eve.principal_id)
            .await,
        Err(WorkspaceError::Forbidden)
    );

    // Removal and message admission serialize on the same chat head.
    let race_message = WorkspaceMessageId::new();
    let (removed, sent) = tokio::join!(
        db.a.remove_workspace_member(&a, chat, bob.principal_id),
        db.b.send_workspace_message(&b, chat, race_message, "Racing removal", None),
    );
    assert!(!removed.unwrap().active);
    match sent {
        Ok(message) => {
            let head = db.a.workspace_events(&a, chat, 0, 100).await.unwrap();
            assert!(message.sequence < head.through_sequence);
        }
        Err(error) => assert_eq!(error, WorkspaceError::NotFound),
    }
    assert_eq!(
        db.b.workspace_snapshot(&b, chat, 0, 100).await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        db.b.workspace_events(&b, chat, 0, 100).await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        db.b.send_workspace_message(&b, chat, WorkspaceMessageId::new(), "After removal", None)
            .await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        db.b.decide_workspace_invitation(&b, chat, invitation, WorkspaceInvitationState::Accepted)
            .await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        db.b.decide_workspace_invitation(
            &b,
            chat,
            older_pending,
            WorkspaceInvitationState::Accepted
        )
        .await,
        Err(WorkspaceError::Conflict)
    );
    assert!(
        db.b.list_workspace_invitations(&b)
            .await
            .unwrap()
            .is_empty()
    );

    // A policy mutation invalidates decisions made before the mutation.
    db.a.client()
        .query("UPDATE ONLY $context SET policy_revision = 'test-v2';")
        .bind(("context", work_context.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        db.a.workspace_snapshot(&a, chat, 0, 100).await,
        Err(WorkspaceError::Forbidden)
    );
    let fresh = authority(&db.a, &alice, "research").await;
    assert!(db.a.workspace_snapshot(&fresh, chat, 0, 100).await.is_ok());
    db.a.client()
        .query("UPDATE ONLY $principal SET enabled = false;")
        .bind(("principal", alice.principal_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        db.a.workspace_snapshot(&fresh, chat, 0, 100).await,
        Err(WorkspaceError::Forbidden)
    );
}

#[tokio::test]
async fn two_writers_have_committed_order_idempotent_messages_and_bounded_replay() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    context(&db.a, &alice, "shared").await;
    let a = authority(&db.a, &alice, "shared").await;
    let b = authority(&db.b, &bob, "shared").await;
    let chat = WorkspaceChatId::new();
    let other = WorkspaceChatId::new();
    db.a.create_workspace_chat(&a, chat, "Collaboration")
        .await
        .unwrap();
    db.a.create_workspace_chat(&a, other, "Private")
        .await
        .unwrap();
    join(&db.a, &a, &b, &bob, chat).await;

    for _ in 0..12 {
        let (one, two) = tokio::join!(
            db.a.send_workspace_message(&a, chat, WorkspaceMessageId::new(), "Alice", None),
            db.b.send_workspace_message(&b, chat, WorkspaceMessageId::new(), "Bob", None),
        );
        assert_ne!(one.unwrap().sequence, two.unwrap().sequence);
    }
    let request = WorkspaceMessageId::new();
    let (one, two) = tokio::join!(
        db.a.send_workspace_message(&a, chat, request, "Retry safely", None),
        db.b.send_workspace_message(&a, chat, request, "Retry safely", None),
    );
    assert_eq!(one.unwrap(), two.unwrap());
    assert_eq!(
        db.a.send_workspace_message(&a, chat, request, "Changed", None)
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        db.b.send_workspace_message(&b, chat, request, "Retry safely", None)
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        db.a.send_workspace_message(
            &a,
            other,
            WorkspaceMessageId::new(),
            "Cross-chat reply",
            Some(request)
        )
        .await,
        Err(WorkspaceError::NotFound)
    );
    let snapshot = db.b.workspace_snapshot(&b, chat, 0, 100).await.unwrap();
    assert_eq!(snapshot.messages.len(), 25);
    let recent =
        db.b.workspace_recent_snapshot(&b, chat, None, 7)
            .await
            .unwrap();
    assert_eq!(recent.messages, snapshot.messages[18..]);
    let before = recent.messages[0].sequence;
    let previous =
        db.b.workspace_recent_snapshot(&b, chat, Some(before), 7)
            .await
            .unwrap();
    assert_eq!(previous.messages, snapshot.messages[11..18]);
    assert!(
        snapshot
            .messages
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
    let authors = snapshot
        .messages
        .iter()
        .map(|message| message.author.to_sql())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(authors.len(), 2);
    let mut cursor = 0;
    loop {
        let page = db.b.workspace_events(&b, chat, cursor, 3).await.unwrap();
        assert!(page.events.len() <= 3);
        for event in page.events {
            assert_eq!(event.sequence, cursor + 1);
            cursor = event.sequence;
        }
        if cursor == page.through_sequence {
            break;
        }
    }
    assert_eq!(cursor, snapshot.chat.sequence);
    assert_eq!(
        db.a.workspace_events(&a, chat, cursor + 1, 100).await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        db.a.workspace_snapshot(&a, chat, 0, 201).await,
        Err(WorkspaceError::Invalid("page"))
    );
}

#[tokio::test]
async fn ownership_settings_and_archive_are_current_and_explicit() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    context(&db.a, &alice, "shared").await;
    let a = authority(&db.a, &alice, "shared").await;
    let b = authority(&db.b, &bob, "shared").await;
    let chat = WorkspaceChatId::new();
    db.a.create_workspace_chat(&a, chat, "Ownership")
        .await
        .unwrap();
    join(&db.a, &a, &b, &bob, chat).await;
    assert_eq!(
        db.a.remove_workspace_member(&a, chat, alice.principal_id)
            .await,
        Err(WorkspaceError::Conflict)
    );
    let snapshot = db.a.workspace_snapshot(&a, chat, 0, 100).await.unwrap();
    db.b.send_workspace_message(
        &b,
        chat,
        WorkspaceMessageId::new(),
        "Settings stay usable",
        None,
    )
    .await
    .unwrap();
    let settings = || WorkspaceSettings {
        expected_revision: snapshot.chat.revision,
        title: "Transferred".into(),
        archived: false,
        members_can_invite: false,
        owner: bob.principal_id,
    };
    assert_eq!(
        db.b.update_workspace_settings(&b, chat, settings()).await,
        Err(WorkspaceError::Forbidden)
    );
    let transferred =
        db.a.update_workspace_settings(&a, chat, settings())
            .await
            .unwrap();
    assert_eq!(transferred.owner, bob.principal_id.record_id());
    assert_eq!(
        db.a.create_workspace_chat(&a, chat, "Ownership")
            .await
            .unwrap()
            .id,
        transferred.id
    );
    assert_eq!(
        db.b.create_workspace_chat(&b, chat, "Ownership").await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        db.a.update_workspace_settings(&a, chat, settings()).await,
        Err(WorkspaceError::Forbidden)
    );
    assert_eq!(
        db.b.update_workspace_settings(&b, chat, settings()).await,
        Err(WorkspaceError::Conflict)
    );
    db.b.update_workspace_settings(
        &b,
        chat,
        WorkspaceSettings {
            expected_revision: transferred.revision,
            archived: true,
            ..settings()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.a.send_workspace_message(&a, chat, WorkspaceMessageId::new(), "Archived", None)
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert!(
        db.a.workspace_snapshot(&a, chat, 0, 100)
            .await
            .unwrap()
            .chat
            .archived
    );
}

#[path = "workspace/runs.rs"]
mod runs;
