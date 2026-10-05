//! Agent revision admission and Workspace binding/import transactions.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "../../../agents/runtime/tests/agent_management/support.rs"]
mod support;
use fixture::TestDb;
use support::*;
use uuid::Uuid;
use veoveo_agent_runtime::persistence::*;
use veoveo_platform_store::{WorkContextMembershipLevel, deterministic_work_context_id};
use veoveo_workspace::persistence::*;

#[tokio::test]
async fn chat_adoption_is_explicit_replayable_and_preserves_running_revision_until_disable() {
    use chrono::{TimeDelta, Utc};
    use veoveo_workspace::persistence::{
        WorkspaceAgentId, WorkspaceAuthority, WorkspaceChatId, WorkspaceError, WorkspaceMessageId,
        WorkspaceRunId, WorkspaceRunState, WorkspaceRunUpdate, WorkspaceTurnRequest,
    };
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
    let identity = identity(&db.a, "chat-registry", "alice").await;
    context(&db.a, &identity, "shared").await;
    let editor = authority(&db.a, &identity, "shared").await;
    let actor = WorkspaceAuthority::new(
        identity.tenant_id,
        deterministic_work_context_id("chat-registry", "shared").unwrap(),
        identity.principal_id,
        editor.context_digest.clone(),
        WorkContextMembershipLevel::Contributor,
    );
    let first = publish(&db.a, &editor, &create(&db.a, &editor, "researcher").await).await;
    let admission = |digest: &str| veoveo_workspace::persistence::WorkspaceAgentAdmission {
        definition: "researcher".into(),
        definition_digest: digest.into(),
        display_name: "Researcher".into(),
        provider: "Fixture".into(),
        model: "Fixture".into(),
    };
    let chat = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&actor, chat, "Explicit revisions")
        .await
        .unwrap();
    let add_request = Uuid::now_v7();
    let agent = WorkspaceRepository::new(db.a.clone())
        .add_workspace_agent(&actor, chat, add_request, admission(&first.draft_digest))
        .await
        .unwrap();
    let agent_id = match &agent.id.key {
        surrealdb::types::RecordIdKey::Uuid(id) => WorkspaceAgentId::from_uuid(**id),
        _ => panic!("agent UUID"),
    };
    let turn = WorkspaceRepository::new(db.a.clone())
        .send_workspace_turn(
            &actor,
            chat,
            WorkspaceTurnRequest {
                id: WorkspaceMessageId::new(),
                text: "Use the original version".into(),
                attachments: vec![],
                reply_to: None,
                addressed_agents: vec![agent_id],
                deadline: Utc::now() + TimeDelta::seconds(120),
            },
        )
        .await
        .unwrap();
    let run = &turn.runs[0];
    let run_id = match &run.id.key {
        surrealdb::types::RecordIdKey::Uuid(id) => WorkspaceRunId::from_uuid(**id),
        _ => panic!("run UUID"),
    };
    let fence = Uuid::now_v7();
    WorkspaceRepository::new(db.a.clone())
        .claim_workspace_run(&actor, chat, run_id, fence)
        .await
        .unwrap();
    let edited = AgentRepository::new(db.a.clone())
        .mutate_agent_definition(
            &editor,
            "researcher",
            Uuid::now_v7(),
            Some(first.revision),
            AgentDefinitionMutation::Draft {
                content: content("Second published instructions"),
            },
        )
        .await
        .unwrap();
    let second = publish(&db.a, &editor, &edited).await;
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_agents(&actor, chat)
            .await
            .unwrap()[0]
            .definition_digest,
        first.draft_digest
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .add_workspace_agent(
                &actor,
                chat,
                Uuid::now_v7(),
                admission(&second.draft_digest)
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    let request = Uuid::now_v7();
    let updated = WorkspaceRepository::new(db.a.clone())
        .update_workspace_agent_revision(
            &actor,
            chat,
            request,
            &first.draft_digest,
            admission(&second.draft_digest),
        )
        .await
        .unwrap();
    assert_eq!(updated.definition_digest, second.draft_digest);
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .update_workspace_agent_revision(
                &actor,
                chat,
                request,
                &first.draft_digest,
                admission(&second.draft_digest)
            )
            .await
            .unwrap(),
        updated
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .update_workspace_agent_revision(
                &actor,
                chat,
                request,
                &first.draft_digest,
                admission(&first.draft_digest)
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .agent_executable(&editor, "researcher", Some(&run.definition_digest))
            .await
            .unwrap()
            .revision
            .content
            .instructions,
        "Private instructions v1"
    );
    WorkspaceRepository::new(db.a.clone())
        .update_workspace_run(
            &actor,
            chat,
            run_id,
            WorkspaceRunUpdate {
                fence,
                text: "Original revision remains active".into(),
                state: WorkspaceRunState::Running,
                feedback: Default::default(),
                failure: None,
            },
        )
        .await
        .unwrap();
    let archived = AgentRepository::new(db.a.clone())
        .mutate_agent_definition(
            &editor,
            "researcher",
            Uuid::now_v7(),
            Some(second.revision),
            AgentDefinitionMutation::Status {
                status: AgentDefinitionStatus::Archived,
            },
        )
        .await
        .unwrap();
    assert!(
        AgentRepository::new(db.a.clone())
            .agent_executable(&editor, "researcher", None)
            .await
            .is_err()
    );
    assert!(
        AgentRepository::new(db.a.clone())
            .agent_executable(&editor, "researcher", Some(&run.definition_digest))
            .await
            .is_ok()
    );
    AgentRepository::new(db.a.clone())
        .mutate_agent_definition(
            &editor,
            "researcher",
            Uuid::now_v7(),
            Some(archived.revision),
            AgentDefinitionMutation::Status {
                status: AgentDefinitionStatus::Disabled,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .update_workspace_run(
                &actor,
                chat,
                run_id,
                WorkspaceRunUpdate {
                    fence,
                    text: "Original revision remains active but cannot publish".into(),
                    state: WorkspaceRunState::Completed,
                    feedback: Default::default(),
                    failure: None
                }
            )
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .workspace_runs(&actor, chat)
            .await
            .unwrap()[0]
            .state,
        WorkspaceRunState::Interrupted
    );
    WorkspaceRepository::new(db.a.clone())
        .remove_workspace_agent(&actor, chat, agent_id)
        .await
        .unwrap();
    // A lost add response cannot reactivate a participant removed after that add.
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .add_workspace_agent(&actor, chat, add_request, admission(&first.draft_digest))
            .await
            .unwrap(),
        agent
    );
    assert!(
        !WorkspaceRepository::new(db.a.clone())
            .workspace_agents(&actor, chat)
            .await
            .unwrap()[0]
            .active
    );
}

#[tokio::test]
async fn offline_chat_import_preserves_identity_and_qualifies_exact_restore() {
    use veoveo_workspace::persistence::{
        WorkspaceAgentAdmission, WorkspaceAuthority, WorkspaceChatId,
    };
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
    let identity = identity(&db.a, "import-fixture", "installer").await;
    context(&db.a, &identity, "shared").await;
    let mut editor = authority(&db.a, &identity, "shared").await;
    editor.manage_context = true;
    let actor = WorkspaceAuthority::new(
        identity.tenant_id,
        deterministic_work_context_id("import-fixture", "shared").unwrap(),
        identity.principal_id,
        editor.context_digest.clone(),
        WorkContextMembershipLevel::Contributor,
    );
    let definition = publish(&db.a, &editor, &create(&db.a, &editor, "researcher").await).await;
    let chat = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&actor, chat, "Retained chat")
        .await
        .unwrap();
    let admitted = WorkspaceRepository::new(db.a.clone())
        .add_workspace_agent(
            &actor,
            chat,
            Uuid::now_v7(),
            WorkspaceAgentAdmission {
                definition: "researcher".into(),
                definition_digest: definition.draft_digest.clone(),
                display_name: "Retained name".into(),
                provider: "Fixture".into(),
                model: "Fixture".into(),
            },
        )
        .await
        .unwrap();
    let source = "b".repeat(64);
    db.a.client()
        .query(include_str!("queries/agent_catalog_integration/offline_chat_import_preserves_identity_and_qualifies_exact_restore/statement_1.surql"))
        .bind(("id", admitted.id.clone()))
        .bind(("digest", source.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let before = WorkspaceRepository::new(db.a.clone())
        .workspace_agents(&actor, chat)
        .await
        .unwrap();
    let mapping = AgentChatImportMapping {
        key: "researcher".into(),
        source_digests: vec![source],
        target_digest: definition.draft_digest.clone(),
    };
    let plan = WorkspaceRepository::new(db.a.clone())
        .plan_agent_chat_import(&editor, std::slice::from_ref(&mapping))
        .await
        .unwrap();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].before, before[0]);
    // The protected recovery file round-trips the typed records exactly.
    let exported = serde_json::to_vec(&plan).unwrap();
    let recovered: Vec<AgentChatImport> = serde_json::from_slice(&exported).unwrap();
    assert_eq!(plan, recovered);
    for _ in 0..2 {
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .apply_agent_chat_import(&editor, &plan, AgentChatImportDirection::Apply)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_agents(&actor, chat)
                .await
                .unwrap()[0],
            admitted
        );
    }
    assert!(
        WorkspaceRepository::new(db.a.clone())
            .plan_agent_chat_import(&editor, &[mapping])
            .await
            .unwrap()
            .is_empty()
    );
    for _ in 0..2 {
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .apply_agent_chat_import(&editor, &recovered, AgentChatImportDirection::Restore)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .workspace_agents(&actor, chat)
                .await
                .unwrap(),
            before
        );
    }
    db.a.client()
        .query(include_str!("queries/agent_catalog_integration/offline_chat_import_preserves_identity_and_qualifies_exact_restore/statement_2.surql"))
        .bind(("id", admitted.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .apply_agent_chat_import(&editor, &plan, AgentChatImportDirection::Apply)
            .await,
        Err(AgentManagementError::Conflict)
    );
    editor.manage_context = false;
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .apply_agent_chat_import(&editor, &plan, AgentChatImportDirection::Apply)
            .await,
        Err(AgentManagementError::Forbidden)
    );
}

fn admission(definition: &AgentDefinition) -> WorkspaceAgentAdmission {
    WorkspaceAgentAdmission {
        definition: definition.key.clone(),
        definition_digest: definition.draft_digest.clone(),
        display_name: definition.name.clone(),
        provider: "Fixture".into(),
        model: "Fixture".into(),
    }
}

async fn import_effects(
    store: &veoveo_platform_store::PlatformStore,
    editor: &AgentCatalogAuthority,
) -> surrealdb::types::Value {
    store
        .client()
        .query(include_str!("queries/agent_admission/snapshot.surql"))
        .bind(("tenant", editor.tenant.clone()))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}

#[tokio::test]
async fn import_rechecks_each_participant_and_revoked_admission_without_partial_effects() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        for (case, mutation, expected) in [
            (
                "participant",
                include_str!("queries/agent_admission/edit_participant.surql"),
                AgentManagementError::Conflict,
            ),
            (
                "disabled",
                include_str!("queries/agent_admission/disable_definition.surql"),
                AgentManagementError::NotFound,
            ),
            (
                "revision",
                include_str!("queries/agent_admission/delete_revision.surql"),
                AgentManagementError::NotFound,
            ),
            (
                "audience",
                include_str!("queries/agent_admission/revoke_audience.surql"),
                AgentManagementError::NotFound,
            ),
            (
                "authority",
                include_str!("queries/agent_admission/revoke_context.surql"),
                AgentManagementError::Forbidden,
            ),
        ] {
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
            let identity = identity(&db.a, &format!("atomic-import-{case}"), "installer").await;
            context(&db.a, &identity, "shared").await;
            let mut editor = authority(&db.a, &identity, "shared").await;
            editor.manage_context = true;
            let actor = WorkspaceAuthority::new(
                identity.tenant_id,
                deterministic_work_context_id(&identity.tenant_key, "shared").unwrap(),
                identity.principal_id,
                editor.context_digest.clone(),
                WorkContextMembershipLevel::Contributor,
            );
            let chat = WorkspaceChatId::new();
            let repository = WorkspaceRepository::new(db.a.clone());
            repository
                .create_workspace_chat(&actor, chat, "Atomic participant import")
                .await
                .unwrap();
            let mut mappings = Vec::new();
            let mut definitions = Vec::new();
            for key in ["first", "second"] {
                let definition = publish(&db.a, &editor, &create(&db.a, &editor, key).await).await;
                let agent = repository
                    .add_workspace_agent(&actor, chat, Uuid::now_v7(), admission(&definition))
                    .await
                    .unwrap();
                let source = "b".repeat(64);
                db.a.client()
                    .query(include_str!(
                        "queries/agent_admission/set_source_digest.surql"
                    ))
                    .bind(("agent", agent.id))
                    .bind(("digest", source.clone()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
                mappings.push(AgentChatImportMapping {
                    key: key.into(),
                    source_digests: vec![source],
                    target_digest: definition.draft_digest.clone(),
                });
                definitions.push(definition);
            }
            let mut plan = repository
                .plan_agent_chat_import(&editor, &mappings)
                .await
                .unwrap();
            plan.sort_by(|left, right| left.before.definition.cmp(&right.before.definition));
            assert_eq!(plan.len(), 2);
            assert_eq!(plan[0].before.definition, "first");
            assert_eq!(plan[1].before.definition, "second");
            // Only the second participant/definition changes, after both planned rows
            // were admitted. The first update must roll back when the second fails.
            db.b.client()
                .query(mutation)
                .bind(("agent", plan[1].before.id.clone()))
                .bind(("definition", definitions[1].id.clone()))
                .bind(("revision", definitions[1].published.clone().unwrap()))
                .bind(("context", editor.work_context.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            let before = import_effects(&db.b, &editor).await;
            assert_eq!(
                repository
                    .apply_agent_chat_import(&editor, &plan, AgentChatImportDirection::Apply)
                    .await,
                Err(expected),
                "{case}"
            );
            assert_eq!(
                import_effects(&db.b, &editor).await,
                before,
                "{case}: participant rows, chat sequence, events and receipts must be unchanged"
            );
        }
    })
    .await
    .expect("atomic import admission exceeded 90 seconds");
}

#[tokio::test]
async fn new_participant_requires_published_head_and_preserves_receipt_replay() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
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
        let identity = identity(&db.a, "new-participant-admission", "alice").await;
        context(&db.a, &identity, "shared").await;
        let editor = authority(&db.a, &identity, "shared").await;
        let actor = WorkspaceAuthority::new(
            identity.tenant_id,
            deterministic_work_context_id(&identity.tenant_key, "shared").unwrap(),
            identity.principal_id,
            editor.context_digest.clone(),
            WorkContextMembershipLevel::Contributor,
        );
        let draft = create(&db.a, &editor, "researcher").await;
        // Keep the audience admissible so rejection specifically exercises the
        // absent published head before the function's typed record conversion.
        db.b.client()
            .query(include_str!(
                "queries/agent_admission/admit_draft_audience.surql"
            ))
            .bind(("definition", draft.id.clone()))
            .bind(("context", editor.work_context.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let repository = WorkspaceRepository::new(db.a.clone());
        let chat = WorkspaceChatId::new();
        repository
            .create_workspace_chat(&actor, chat, "Published participant admission")
            .await
            .unwrap();
        let before = import_effects(&db.b, &editor).await;
        let request = Uuid::now_v7();
        assert_eq!(
            repository
                .add_workspace_agent(&actor, chat, request, admission(&draft))
                .await,
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(import_effects(&db.b, &editor).await, before);
        let published = publish(&db.a, &editor, &draft).await;
        let added = repository
            .add_workspace_agent(&actor, chat, request, admission(&published))
            .await
            .unwrap();
        let committed = import_effects(&db.b, &editor).await;
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .add_workspace_agent(&actor, chat, request, admission(&published))
                .await
                .unwrap(),
            added
        );
        assert_eq!(
            import_effects(&db.b, &editor).await,
            committed,
            "receipt replay must not add another participant, event or chat sequence"
        );
        assert_eq!(
            repository.workspace_agents(&actor, chat).await.unwrap(),
            vec![added]
        );
    })
    .await
    .expect("new participant admission exceeded 60 seconds");
}
