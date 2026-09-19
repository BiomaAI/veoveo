//! Real database checks for publication, authority fencing and concurrent authoring.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use fixture::TestDb;
use uuid::Uuid;
use veoveo_platform_store::{
    PlatformIdentity, PlatformStore, PrincipalKind, WorkContextMembershipLevel,
    agent_management::*, deterministic_work_context_id,
};

fn content(instructions: &str) -> AgentContent {
    AgentContent {
        model: AgentModelReference {
            id: "approved-model".into(),
            revision: "a".repeat(64),
        },
        instructions: instructions.into(),
        tools: vec!["time__resolve_time".into()],
        budgets: AgentBudgets {
            max_output_tokens: 4096,
            max_completion_calls: 4,
            max_tool_calls: 8,
            deadline_seconds: 120,
        },
        execution: AgentExecution::Chat,
    }
}

async fn identity(store: &PlatformStore, tenant: &str, key: &str) -> PlatformIdentity {
    store
        .ensure_identity(
            tenant,
            key,
            "https://identity.test",
            key,
            PrincipalKind::User,
        )
        .await
        .unwrap()
}

async fn context(store: &PlatformStore, actor: &PlatformIdentity, key: &str) {
    let id = deterministic_work_context_id(&actor.tenant_key, key).unwrap();
    store.client().query("CREATE ONLY $context SET tenant = $tenant, context_key = $key, title = $key, policy_revision = 'test-v1', memberships = [], output_policy = {owner_kind: 'principal', owner_key: 'alice', initial_grants: [], data_labels: []};")
        .bind(("context", id.record_id())).bind(("tenant", actor.tenant_id.record_id())).bind(("key", key.to_owned()))
        .await.unwrap().check().unwrap();
}

async fn authority(
    store: &PlatformStore,
    actor: &PlatformIdentity,
    key: &str,
) -> AgentCatalogAuthority {
    let version = store
        .artifact_read_context_version(&actor.tenant_key, key)
        .await
        .unwrap()
        .unwrap();
    AgentCatalogAuthority::new(
        actor.tenant_id,
        deterministic_work_context_id(&actor.tenant_key, key).unwrap(),
        actor.principal_id,
        version.digest,
        WorkContextMembershipLevel::Contributor,
        false,
        100,
    )
}

fn audience(actor: &AgentCatalogAuthority) -> Vec<AgentPublicationContext> {
    vec![AgentPublicationContext {
        work_context: actor.work_context.clone(),
        context_digest: actor.context_digest.clone(),
    }]
}

async fn create(
    store: &PlatformStore,
    actor: &AgentCatalogAuthority,
    key: &str,
) -> AgentDefinition {
    store
        .mutate_agent_definition(
            actor,
            key,
            Uuid::now_v7(),
            None,
            AgentDefinitionMutation::Create {
                name: "Research assistant".into(),
                description: "An isolated test agent".into(),
                content: content("Private instructions v1"),
            },
        )
        .await
        .unwrap()
}

async fn publish(
    store: &PlatformStore,
    actor: &AgentCatalogAuthority,
    definition: &AgentDefinition,
) -> AgentDefinition {
    store
        .mutate_agent_definition(
            actor,
            &definition.key,
            Uuid::now_v7(),
            Some(definition.revision),
            AgentDefinitionMutation::Publish {
                digest: definition.draft.digest().unwrap(),
                audience: audience(actor),
            },
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn publication_pins_content_and_catalog_never_discloses_instructions() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "catalog-test", "alice").await;
    let bob = identity(&db.a, "catalog-test", "bob").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let b = authority(&db.b, &bob, "research").await;
    let initial = create(&db.a, &a, "researcher").await;
    assert!(db.b.agent_catalog(&b, None, 20).await.unwrap().is_empty());
    assert_eq!(
        db.b.agent_definition(&b, "researcher").await,
        Err(AgentManagementError::NotFound)
    );
    let first = publish(&db.a, &a, &initial).await;
    assert!(first.published.is_some());
    assert_eq!(first.audience, vec![a.work_context.clone()]);
    let catalog = db.b.agent_catalog(&b, None, 20).await.unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].digest, initial.draft.digest().unwrap());
    let immutable =
        db.a.client()
            .query("UPDATE ONLY $revision SET content.instructions = 'tampered';")
            .bind(("revision", first.published.clone().unwrap()))
            .await
            .unwrap()
            .check();
    assert!(
        immutable.is_err(),
        "published executable content must be immutable"
    );
    let public = serde_json::to_string(&catalog).unwrap();
    assert!(!public.contains("instructions"));
    assert!(!public.contains("Private instructions"));
    let old_digest = catalog[0].digest.clone();
    let draft =
        db.b.mutate_agent_definition(
            &a,
            "researcher",
            Uuid::now_v7(),
            Some(first.revision),
            AgentDefinitionMutation::Draft {
                content: content("Private instructions v2"),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        db.a.agent_catalog(&b, None, 20).await.unwrap()[0].digest,
        old_digest
    );
    let forged_digest =
        db.a.mutate_agent_definition(
            &a,
            "researcher",
            Uuid::now_v7(),
            Some(draft.revision),
            AgentDefinitionMutation::Publish {
                digest: old_digest.clone(),
                audience: audience(&a),
            },
        )
        .await;
    assert_eq!(forged_digest, Err(AgentManagementError::Conflict));
    let second = publish(&db.a, &a, &draft).await;
    assert_ne!(
        db.b.agent_catalog(&b, None, 20).await.unwrap()[0].digest,
        old_digest
    );
    assert_eq!(
        db.b.agent_revision(&b, "researcher", &old_digest)
            .await
            .unwrap()
            .content,
        initial.draft
    );
    let archived =
        db.a.mutate_agent_definition(
            &a,
            "researcher",
            Uuid::now_v7(),
            Some(second.revision),
            AgentDefinitionMutation::Status {
                status: AgentDefinitionStatus::Archived,
            },
        )
        .await
        .unwrap();
    assert!(db.b.agent_catalog(&b, None, 20).await.unwrap().is_empty());
    assert!(
        db.b.agent_revision(&b, "researcher", &old_digest)
            .await
            .is_ok()
    );
    db.a.mutate_agent_definition(
        &a,
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
        db.b.agent_revision(&b, "researcher", &old_digest).await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        db.a.agent_revision_history(&a, "researcher")
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn mutation_replay_conflicts_and_concurrent_edits_preserve_one_result() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "catalog-test", "alice").await;
    context(&db.a, &alice, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let request = Uuid::now_v7();
    let create = AgentDefinitionMutation::Create {
        name: "Original".into(),
        description: "A test definition".into(),
        content: content("Instruction"),
    };
    let (first, replay) = tokio::join!(
        db.a.mutate_agent_definition(&a, "writer", request, None, create.clone()),
        db.b.mutate_agent_definition(&a, "writer", request, None, create.clone())
    );
    let first = first.unwrap();
    assert_eq!(replay.unwrap(), first);
    let (left, right) = tokio::join!(
        db.a.mutate_agent_definition(
            &a,
            "writer",
            Uuid::now_v7(),
            Some(first.revision),
            AgentDefinitionMutation::Draft {
                content: content("Left")
            }
        ),
        db.b.mutate_agent_definition(
            &a,
            "writer",
            Uuid::now_v7(),
            Some(first.revision),
            AgentDefinitionMutation::Draft {
                content: content("Right")
            }
        )
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(
        left == Err(AgentManagementError::Conflict) || right == Err(AgentManagementError::Conflict)
    );
    assert_eq!(
        db.a.mutate_agent_definition(&a, "writer", request, None, create)
            .await
            .unwrap(),
        first
    );
    assert_eq!(
        db.b.mutate_agent_definition(
            &a,
            "another",
            request,
            None,
            AgentDefinitionMutation::Create {
                name: "Changed".into(),
                description: "Different payload".into(),
                content: content("New")
            }
        )
        .await,
        Err(AgentManagementError::Conflict)
    );
    let events =
        db.a.client()
            .query("SELECT * FROM outbox_event WHERE aggregate_type = 'agent_definition';")
            .await
            .unwrap()
            .check()
            .unwrap()
            .take::<Vec<veoveo_platform_store::OutboxEventRecord>>(0)
            .unwrap();
    assert_eq!(events.len(), 2);
    assert!(
        !serde_json::to_string(&events)
            .unwrap()
            .contains("Instruction")
    );
}

#[tokio::test]
async fn current_context_identity_and_publication_audience_are_fenced() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "catalog-test", "alice").await;
    let outsider = identity(&db.a, "other-tenant", "outsider").await;
    context(&db.a, &alice, "research").await;
    context(&db.a, &alice, "private").await;
    context(&db.a, &outsider, "research").await;
    let a = authority(&db.a, &alice, "research").await;
    let private = authority(&db.a, &alice, "private").await;
    let outside = authority(&db.a, &outsider, "research").await;
    let created = create(&db.a, &a, "assistant").await;
    let mut viewer = a.clone();
    viewer.membership = WorkContextMembershipLevel::Viewer;
    assert_eq!(
        db.b.mutate_agent_definition(
            &viewer,
            "assistant",
            Uuid::now_v7(),
            Some(1),
            AgentDefinitionMutation::Draft {
                content: content("Denied")
            }
        )
        .await,
        Err(AgentManagementError::Forbidden)
    );
    assert_eq!(
        db.b.agent_definition(&private, "assistant").await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        db.b.agent_definition(&outside, "assistant").await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        db.b.mutate_agent_definition(
            &a,
            "assistant",
            Uuid::now_v7(),
            Some(1),
            AgentDefinitionMutation::Publish {
                digest: created.draft_digest.clone(),
                audience: audience(&outside)
            }
        )
        .await,
        Err(AgentManagementError::Forbidden)
    );
    db.a.client()
        .query("UPDATE ONLY $context SET policy_revision = 'test-v2';")
        .bind(("context", private.work_context.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        db.b.mutate_agent_definition(
            &a,
            "assistant",
            Uuid::now_v7(),
            Some(1),
            AgentDefinitionMutation::Publish {
                digest: created.draft_digest.clone(),
                audience: audience(&private)
            }
        )
        .await,
        Err(AgentManagementError::Forbidden)
    );
    publish(&db.a, &a, &created).await;
    db.a.client()
        .query("UPDATE ONLY $principal SET enabled = false;")
        .bind(("principal", alice.principal_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        db.b.agent_catalog(&a, None, 10).await,
        Err(AgentManagementError::Forbidden)
    );
}

#[tokio::test]
async fn capacity_is_atomic_across_replicas_and_retry_does_not_reserve_twice() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "catalog-test", "alice").await;
    context(&db.a, &alice, "research").await;
    let mut a = authority(&db.a, &alice, "research").await;
    a.definition_limit = 1;
    let mutation = AgentDefinitionMutation::Create {
        name: "Only one".into(),
        description: "Capacity fixture".into(),
        content: content("Bounded"),
    };
    let (first, second) = tokio::join!(
        db.a.mutate_agent_definition(&a, "one", Uuid::now_v7(), None, mutation.clone()),
        db.b.mutate_agent_definition(&a, "two", Uuid::now_v7(), None, mutation)
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert!(
        first == Err(AgentManagementError::Capacity)
            || second == Err(AgentManagementError::Capacity)
    );
    assert_eq!(db.a.agent_definitions(&a, None, 20).await.unwrap().len(), 1);
}

#[test]
fn executable_input_rejects_unknown_secret_and_unbounded_fields() {
    let mut value = serde_json::to_value(content("Safe instructions")).unwrap();
    value["model"]["api_key"] = serde_json::json!("must-not-be-accepted");
    assert!(serde_json::from_value::<AgentContent>(value).is_err());
    let mut invalid = content("Instructions");
    invalid.tools.push(invalid.tools[0].clone());
    assert_eq!(
        invalid.validate(),
        Err(AgentManagementError::Invalid("tools"))
    );
    invalid = content("Instructions");
    invalid.budgets.max_completion_calls = 1000;
    assert_eq!(
        invalid.validate(),
        Err(AgentManagementError::Invalid("budgets"))
    );
    let mut managed = content("Reviewed template");
    managed.execution = AgentExecution::Managed {
        template: "pilot".into(),
        template_revision: "b".repeat(64),
        parameters: std::collections::BTreeMap::from([
            (
                "vehicle".into(),
                AgentTemplateParameter::Text("uav-1".into()),
            ),
            ("enabled".into(), AgentTemplateParameter::Boolean(true)),
            ("count".into(), AgentTemplateParameter::Integer(1)),
        ]),
        resource_subscriptions: vec!["uav-sim://session/test".into()],
    };
    assert!(managed.validate().is_ok());
    use surrealdb::types::SurrealValue;
    assert_eq!(
        AgentContent::from_value(managed.clone().into_value()).unwrap(),
        managed
    );
}
