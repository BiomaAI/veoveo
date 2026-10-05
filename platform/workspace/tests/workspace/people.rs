use super::*;

async fn fixture() -> TestDb {
    TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
        veoveo_workspace::schema::module_setup(
            fixture::module_lanes::execution("workspace").unwrap(),
        )
        .unwrap(),
    ])
    .await
}

async fn configure(store: &PlatformStore, identity: &PlatformIdentity, enabled: bool, kind: &str) {
    store
        .client()
        .query(include_str!("../queries/workspace/people/configure.surql"))
        .bind(("principal", identity.principal_id.record_id()))
        .bind(("enabled", enabled))
        .bind(("kind", kind.to_owned()))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn people_search_and_identity_preserve_normalization_and_tenant_filters() {
    let db = fixture().await;
    let alice = identity(&db.a, "alice").await;
    context(&db.a, &alice, "people").await;
    let a = authority(&db.a, &alice, "people").await;
    let repo = WorkspaceRepository::new(db.a.clone());
    let labels = repo.workspace_identity(&a).await.unwrap();
    assert_eq!(labels.person.id, alice.principal_id.record_id());
    assert_eq!(labels.person.display_name, "alice");
    assert_eq!(labels.work_context_title, "people");
    let tenant: veoveo_platform_store::TenantRecord =
        db.a.client()
            .select(alice.tenant_id.record_id())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(labels.tenant_name, tenant.name);
    for (tenant, key, name, kind, enabled) in [
        (
            "workspace-test",
            "disabled",
            "00 Match disabled",
            PrincipalKind::User,
            false,
        ),
        (
            "workspace-test",
            "service",
            "00 Match service",
            PrincipalKind::Service,
            true,
        ),
        (
            "workspace-foreign",
            "foreign",
            "00 Match foreign",
            PrincipalKind::User,
            true,
        ),
    ] {
        let actor =
            db.a.ensure_named_identity(tenant, key, "https://identity.test", key, kind, name)
                .await
                .unwrap();
        if !enabled {
            configure(&db.a, &actor, false, "user").await;
        }
    }
    for index in (0..25).rev() {
        let key = format!("match-{index}");
        db.a.ensure_named_identity(
            "workspace-test",
            &key,
            "https://identity.test",
            &key,
            PrincipalKind::User,
            &format!("Match {index:02}"),
        )
        .await
        .unwrap();
    }
    let found = repo.search_workspace_people(&a, "  MaTcH  ").await.unwrap();
    assert_eq!(
        found
            .iter()
            .map(|p| p.display_name.clone())
            .collect::<Vec<_>>(),
        (0..20)
            .map(|index| format!("Match {index:02}"))
            .collect::<Vec<_>>()
    );
    for invalid in ["", " a ", &"x".repeat(129), &"é".repeat(65)] {
        assert_eq!(
            repo.search_workspace_people(&a, invalid).await,
            Err(WorkspaceError::Invalid("people search"))
        );
    }
}

#[tokio::test]
async fn invitations_admit_only_enabled_tenant_humans_and_failed_admission_rolls_back() {
    let db = fixture().await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    let disabled = identity(&db.a, "disabled").await;
    let service = identity(&db.a, "service").await;
    let foreign =
        db.a.ensure_identity(
            "workspace-foreign",
            "foreign",
            "https://identity.test",
            "foreign",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    configure(&db.a, &disabled, false, "user").await;
    configure(&db.a, &service, true, "service").await;
    context(&db.a, &alice, "invitations").await;
    let a = authority(&db.a, &alice, "invitations").await;
    let b = authority(&db.a, &bob, "invitations").await;
    let repo = WorkspaceRepository::new(db.a.clone());
    let chat = WorkspaceChatId::new();
    repo.create_workspace_chat(&a, chat, "Invitation policy")
        .await
        .unwrap();
    let before = repo.workspace_head(&a, chat).await.unwrap();
    for principal in [
        disabled.principal_id,
        service.principal_id,
        foreign.principal_id,
    ] {
        let invitation = WorkspaceInvitationId::new();
        assert_eq!(
            repo.invite_workspace_member(&a, chat, invitation, principal)
                .await,
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(repo.workspace_head(&a, chat).await.unwrap(), before);
        assert!(
            repo.list_workspace_invitations(&a)
                .await
                .unwrap()
                .is_empty()
        );
        let row: Option<veoveo_workspace::persistence::WorkspaceInvitation> =
            db.a.client().select(invitation.record_id()).await.unwrap();
        assert!(row.is_none());
    }
    let invitation = WorkspaceInvitationId::new();
    repo.invite_workspace_member(&a, chat, invitation, bob.principal_id)
        .await
        .unwrap();
    assert_eq!(repo.workspace_head(&a, chat).await.unwrap(), before + 1);
    let inbox = repo.workspace_invitation_inbox(&b).await.unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].chat_title, "Invitation policy");
    assert_eq!(inbox[0].inviter_name, "alice");
    assert!(
        repo.workspace_invitation_inbox(&a)
            .await
            .unwrap()
            .is_empty()
    );
    configure(&db.a, &alice, false, "user").await;
    assert_eq!(
        repo.workspace_invitation_inbox(&b).await.unwrap()[0].inviter_name,
        "alice"
    );
    db.a.client()
        .query(include_str!("../queries/workspace/people/expire.surql"))
        .bind(("invitation", invitation.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        repo.workspace_invitation_inbox(&b)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn member_names_keep_disabled_history_and_exclude_foreign_principals() {
    let db = fixture().await;
    let alice = identity(&db.a, "alice").await;
    let bob = identity(&db.a, "bob").await;
    let foreign =
        db.a.ensure_identity(
            "workspace-foreign",
            "foreign",
            "https://identity.test",
            "foreign",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    context(&db.a, &alice, "members").await;
    let a = authority(&db.a, &alice, "members").await;
    let b = authority(&db.a, &bob, "members").await;
    let repo = WorkspaceRepository::new(db.a.clone());
    let chat = WorkspaceChatId::new();
    repo.create_workspace_chat(&a, chat, "Names").await.unwrap();
    join(&db.a, &a, &b, &bob, chat).await;
    configure(&db.a, &bob, false, "user").await;
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
    let names = repo.workspace_member_people(&a, chat).await.unwrap();
    assert_eq!(names.len(), 2);
    assert!(
        names
            .iter()
            .any(|p| p.id == bob.principal_id.record_id() && p.display_name == "bob")
    );
    assert!(
        !names
            .iter()
            .any(|p| p.id == foreign.principal_id.record_id())
    );
    assert_eq!(
        repo.workspace_member_people(&a, WorkspaceChatId::new())
            .await,
        Err(WorkspaceError::NotFound)
    );
}

#[tokio::test]
async fn member_name_input_keeps_the_workspace_256_member_cap() {
    let db = fixture().await;
    let alice = identity(&db.a, "alice").await;
    context(&db.a, &alice, "capacity").await;
    let a = authority(&db.a, &alice, "capacity").await;
    let repo = WorkspaceRepository::new(db.a.clone());
    let chat = WorkspaceChatId::new();
    repo.create_workspace_chat(&a, chat, "Member capacity")
        .await
        .unwrap();
    db.a.client()
        .query(include_str!(
            "../queries/workspace/people/member_capacity.surql"
        ))
        .bind(("tenant", alice.tenant_id.record_id()))
        .bind(("chat", chat.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        repo.workspace_member_people(&a, chat).await.unwrap().len(),
        256
    );
}
