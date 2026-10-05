//! Actual database acceptance for the shared conversation boundary. The fixture
//! owns a disposable store and never uses installation identities or data.
use veoveo_workspace::persistence::WorkspaceRepository;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use fixture::TestDb;
use surrealdb::types::ToSql;
use veoveo_platform_store::{
    PlatformIdentity, PlatformStore, PrincipalKind, WorkContextId, WorkContextMembershipLevel,
    deterministic_work_context_id,
};
use veoveo_workspace::persistence::{
    WorkspaceAuthority, WorkspaceChatId, WorkspaceError, WorkspaceInvitationId,
    WorkspaceInvitationState, WorkspaceMessageId, WorkspaceSettings,
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
    store
        .client()
        .query(include_str!("queries/workspace/context/statement_1.surql"))
        .bind(("context", context.record_id()))
        .bind(("tenant", identity.tenant_id.record_id()))
        .bind(("key", key.to_owned()))
        .await
        .unwrap()
        .check()
        .unwrap();
    // Explicit registry fixtures keep Workspace tests focused on chat transactions.
    // Executable digests come from the same typed content, not synthetic revisions.
    let content = runs::catalog_content();
    let digest = content.digest().unwrap();
    for key in ["writer", "reviewer", "helper", "researcher"] {
        let definition = veoveo_agent_runtime::persistence::agent_definition_record(
            &identity.tenant_id.record_id(),
            key,
        )
        .unwrap();
        store
            .client()
            .query(include_str!("queries/workspace/context/statement_2.surql"))
            .bind((
                "revision",
                surrealdb::types::RecordId::new(
                    "agent_definition_revision",
                    surrealdb::types::Uuid::from(uuid::Uuid::new_v5(
                        &uuid::Uuid::NAMESPACE_OID,
                        format!("{}:{digest}", definition.to_sql()).as_bytes(),
                    )),
                ),
            ))
            .bind(("definition", definition))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("context", context.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("key", key.to_owned()))
            .bind(("content", content.clone()))
            .bind(("digest", digest.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
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
    WorkspaceRepository::new(store.clone())
        .invite_workspace_member(alice, chat, invitation, identity.principal_id)
        .await
        .unwrap();
    WorkspaceRepository::new(store.clone())
        .decide_workspace_invitation(bob, chat, invitation, WorkspaceInvitationState::Accepted)
        .await
        .unwrap();
    invitation
}

#[tokio::test]
async fn invitations_require_the_named_current_human_and_removal_revokes_history() {
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
    let work_context = context(&db.a, &alice, "research").await;
    context(&db.a, &alice, "private").await;
    let a = authority(&db.a, &alice, "research").await;
    let b = authority(&db.b, &bob, "research").await;
    let e = authority(&db.b, &eve, "research").await;
    let private = authority(&db.a, &alice, "private").await;
    let chat = WorkspaceChatId::new();
    let created = WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, chat, "Research")
        .await
        .unwrap();
    assert_eq!(created.sequence, 1);
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .create_workspace_chat(&a, chat, "Research")
            .await
            .unwrap(),
        created
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .create_workspace_chat(&a, chat, "Changed")
            .await,
        Err(WorkspaceError::Conflict)
    );
    for denied in [&b, &e, &private] {
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_snapshot(denied, chat, 0, 100)
                .await,
            Err(WorkspaceError::NotFound)
        );
        assert!(
            WorkspaceRepository::new(db.b.clone())
                .list_workspace_chats(denied)
                .await
                .unwrap()
                .is_empty()
        );
    }
    let original = WorkspaceRepository::new(db.a.clone())
        .send_workspace_turn(
            &a,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: WorkspaceMessageId::new(),
                text: ("Shared history").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
    let invitation = WorkspaceInvitationId::new();
    WorkspaceRepository::new(db.a.clone())
        .invite_workspace_member(&a, chat, invitation, bob.principal_id)
        .await
        .unwrap();
    let older_pending = WorkspaceInvitationId::new();
    WorkspaceRepository::new(db.a.clone())
        .invite_workspace_member(&a, chat, older_pending, bob.principal_id)
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .list_workspace_invitations(&b)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(
        WorkspaceRepository::new(db.b.clone())
            .list_workspace_invitations(&e)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .decide_workspace_invitation(&e, chat, invitation, WorkspaceInvitationState::Accepted)
            .await,
        Err(WorkspaceError::NotFound)
    );
    WorkspaceRepository::new(db.b.clone())
        .decide_workspace_invitation(&b, chat, invitation, WorkspaceInvitationState::Accepted)
        .await
        .unwrap();
    let snapshot = WorkspaceRepository::new(db.b.clone())
        .workspace_snapshot(&b, chat, 0, 100)
        .await
        .unwrap();
    assert_eq!(snapshot.messages, [original.message]);
    assert_eq!(snapshot.members.len(), 2);
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .invite_workspace_member(&b, chat, WorkspaceInvitationId::new(), eve.principal_id)
            .await,
        Err(WorkspaceError::Forbidden)
    );

    // Removal and message admission serialize on the same chat head.
    let race_message = WorkspaceMessageId::new();
    let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
    let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
    let (removed, sent) = tokio::join!(
        workspace_repository_a.remove_workspace_member(&a, chat, bob.principal_id),
        workspace_repository_b.send_workspace_turn(
            &b,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: race_message,
                text: ("Racing removal").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
            }
        ),
    );
    assert!(!removed.unwrap().active);
    match sent {
        Ok(message) => {
            let head = WorkspaceRepository::new(db.a.clone())
                .workspace_events(&a, chat, 0, 100)
                .await
                .unwrap();
            assert!(message.message.sequence < head.through_sequence);
        }
        Err(error) => assert_eq!(error, WorkspaceError::NotFound),
    }
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .workspace_snapshot(&b, chat, 0, 100)
            .await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .workspace_events(&b, chat, 0, 100)
            .await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .send_workspace_turn(
                &b,
                chat,
                veoveo_workspace::persistence::WorkspaceTurnRequest {
                    id: WorkspaceMessageId::new(),
                    text: ("After removal").to_owned(),
                    reply_to: None,
                    attachments: vec![],
                    addressed_agents: vec![],
                    deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
                }
            )
            .await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .decide_workspace_invitation(&b, chat, invitation, WorkspaceInvitationState::Accepted)
            .await,
        Err(WorkspaceError::NotFound)
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .decide_workspace_invitation(
                &b,
                chat,
                older_pending,
                WorkspaceInvitationState::Accepted
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert!(
        WorkspaceRepository::new(db.b.clone())
            .list_workspace_invitations(&b)
            .await
            .unwrap()
            .is_empty()
    );

    // A policy mutation invalidates decisions made before the mutation.
    db.a.client()
        .query(include_str!("queries/workspace/invitations_require_the_named_current_human_and_removal_revokes_history/statement_1.surql"))
        .bind(("context", work_context.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_snapshot(&a, chat, 0, 100)
            .await,
        Err(WorkspaceError::Forbidden)
    );
    let fresh = authority(&db.a, &alice, "research").await;
    assert!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_snapshot(&fresh, chat, 0, 100)
            .await
            .is_ok()
    );
    db.a.client()
        .query(include_str!("queries/workspace/invitations_require_the_named_current_human_and_removal_revokes_history/statement_2.surql"))
        .bind(("principal", alice.principal_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_snapshot(&fresh, chat, 0, 100)
            .await,
        Err(WorkspaceError::Forbidden)
    );
}

#[tokio::test]
async fn two_writers_have_committed_order_idempotent_messages_and_bounded_replay() {
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
        .create_workspace_chat(&a, chat, "Collaboration")
        .await
        .unwrap();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, other, "Private")
        .await
        .unwrap();
    join(&db.a, &a, &b, &bob, chat).await;

    for _ in 0..12 {
        let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
        let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
        let (one, two) = tokio::join!(
            workspace_repository_a.send_workspace_turn(
                &a,
                chat,
                veoveo_workspace::persistence::WorkspaceTurnRequest {
                    id: WorkspaceMessageId::new(),
                    text: ("Alice").to_owned(),
                    reply_to: None,
                    attachments: vec![],
                    addressed_agents: vec![],
                    deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
                }
            ),
            workspace_repository_b.send_workspace_turn(
                &b,
                chat,
                veoveo_workspace::persistence::WorkspaceTurnRequest {
                    id: WorkspaceMessageId::new(),
                    text: ("Bob").to_owned(),
                    reply_to: None,
                    attachments: vec![],
                    addressed_agents: vec![],
                    deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
                }
            ),
        );
        assert_ne!(one.unwrap().message.sequence, two.unwrap().message.sequence);
    }
    let request = WorkspaceMessageId::new();
    let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
    let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
    let (one, two) = tokio::join!(
        workspace_repository_a.send_workspace_turn(
            &a,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: request,
                text: ("Retry safely").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
            }
        ),
        workspace_repository_b.send_workspace_turn(
            &a,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: request,
                text: ("Retry safely").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
            }
        ),
    );
    assert_eq!(one.unwrap(), two.unwrap());
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(
                &a,
                chat,
                veoveo_workspace::persistence::WorkspaceTurnRequest {
                    id: request,
                    text: ("Changed").to_owned(),
                    reply_to: None,
                    attachments: vec![],
                    addressed_agents: vec![],
                    deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
                }
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .send_workspace_turn(
                &b,
                chat,
                veoveo_workspace::persistence::WorkspaceTurnRequest {
                    id: request,
                    text: ("Retry safely").to_owned(),
                    reply_to: None,
                    attachments: vec![],
                    addressed_agents: vec![],
                    deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
                }
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(
                &a,
                other,
                veoveo_workspace::persistence::WorkspaceTurnRequest {
                    id: WorkspaceMessageId::new(),
                    text: ("Cross-chat reply").to_owned(),
                    reply_to: Some(
                        veoveo_workspace::persistence::WorkspaceReplyTarget::Message(request)
                    ),
                    attachments: vec![],
                    addressed_agents: vec![],
                    deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
                }
            )
            .await,
        Err(WorkspaceError::NotFound)
    );
    let snapshot = WorkspaceRepository::new(db.b.clone())
        .workspace_snapshot(&b, chat, 0, 100)
        .await
        .unwrap();
    assert_eq!(snapshot.messages.len(), 25);
    let recent = WorkspaceRepository::new(db.b.clone())
        .workspace_recent_snapshot(&b, chat, None, 7)
        .await
        .unwrap();
    assert_eq!(recent.messages, snapshot.messages[18..]);
    let before = recent.messages[0].sequence;
    let previous = WorkspaceRepository::new(db.b.clone())
        .workspace_recent_snapshot(&b, chat, Some(before), 7)
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
        let page = WorkspaceRepository::new(db.b.clone())
            .workspace_events(&b, chat, cursor, 3)
            .await
            .unwrap();
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
        WorkspaceRepository::new(db.a.clone())
            .workspace_events(&a, chat, cursor + 1, 100)
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_snapshot(&a, chat, 0, 201)
            .await,
        Err(WorkspaceError::Invalid("page"))
    );
}

#[tokio::test]
async fn ownership_settings_and_archive_are_current_and_explicit() {
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
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, chat, "Ownership")
        .await
        .unwrap();
    join(&db.a, &a, &b, &bob, chat).await;
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .remove_workspace_member(&a, chat, alice.principal_id)
            .await,
        Err(WorkspaceError::Conflict)
    );
    let snapshot = WorkspaceRepository::new(db.a.clone())
        .workspace_snapshot(&a, chat, 0, 100)
        .await
        .unwrap();
    WorkspaceRepository::new(db.b.clone())
        .send_workspace_turn(
            &b,
            chat,
            veoveo_workspace::persistence::WorkspaceTurnRequest {
                id: WorkspaceMessageId::new(),
                text: ("Settings stay usable").to_owned(),
                reply_to: None,
                attachments: vec![],
                addressed_agents: vec![],
                deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
    let settings = || WorkspaceSettings {
        expected_revision: snapshot.chat.revision,
        title: "Transferred".into(),
        archived: false,
        members_can_invite: false,
        owner: bob.principal_id,
        participation: Default::default(),
    };
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .update_workspace_settings(&b, chat, settings())
            .await,
        Err(WorkspaceError::Forbidden)
    );
    let transferred = WorkspaceRepository::new(db.a.clone())
        .update_workspace_settings(&a, chat, settings())
        .await
        .unwrap();
    assert_eq!(transferred.owner, bob.principal_id.record_id());
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .create_workspace_chat(&a, chat, "Ownership")
            .await
            .unwrap()
            .id,
        transferred.id
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .create_workspace_chat(&b, chat, "Ownership")
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .update_workspace_settings(&a, chat, settings())
            .await,
        Err(WorkspaceError::Forbidden)
    );
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .update_workspace_settings(&b, chat, settings())
            .await,
        Err(WorkspaceError::Conflict)
    );
    WorkspaceRepository::new(db.b.clone())
        .update_workspace_settings(
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
        WorkspaceRepository::new(db.a.clone())
            .send_workspace_turn(
                &a,
                chat,
                veoveo_workspace::persistence::WorkspaceTurnRequest {
                    id: WorkspaceMessageId::new(),
                    text: ("Archived").to_owned(),
                    reply_to: None,
                    attachments: vec![],
                    addressed_agents: vec![],
                    deadline: chrono::Utc::now() + chrono::TimeDelta::seconds(120)
                }
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_snapshot(&a, chat, 0, 100)
            .await
            .unwrap()
            .chat
            .archived
    );
}

#[path = "workspace/runs.rs"]
mod runs;

#[path = "workspace/operations.rs"]
mod operations;

#[path = "workspace/participation.rs"]
mod participation;

#[path = "workspace/replies.rs"]
mod replies;

#[path = "workspace/attachments.rs"]
mod attachments;

#[path = "workspace/personal.rs"]
mod personal;

#[path = "workspace/people.rs"]
mod people;
