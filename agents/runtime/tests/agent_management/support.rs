use uuid::Uuid;
use veoveo_agent_runtime::persistence::*;
use veoveo_platform_store::{
    PlatformIdentity, PlatformStore, PrincipalKind, WorkContextMembershipLevel,
    deterministic_work_context_id,
};

pub fn content(instructions: &str) -> AgentContent {
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

pub async fn identity(store: &PlatformStore, tenant: &str, key: &str) -> PlatformIdentity {
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

pub async fn context(store: &PlatformStore, actor: &PlatformIdentity, key: &str) {
    let id = deterministic_work_context_id(&actor.tenant_key, key).unwrap();
    store
        .client()
        .query(include_str!(
            "../queries/agent_management/support/context/statement_1.surql"
        ))
        .bind(("context", id.record_id()))
        .bind(("tenant", actor.tenant_id.record_id()))
        .bind(("key", key.to_owned()))
        .await
        .unwrap()
        .check()
        .unwrap();
}

pub async fn authority(
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

pub fn audience(actor: &AgentCatalogAuthority) -> Vec<AgentPublicationContext> {
    vec![AgentPublicationContext {
        work_context: actor.work_context.clone(),
        context_digest: actor.context_digest.clone(),
    }]
}

pub async fn create(
    store: &PlatformStore,
    actor: &AgentCatalogAuthority,
    key: &str,
) -> AgentDefinition {
    AgentRepository::new(store.clone())
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

pub async fn publish(
    store: &PlatformStore,
    actor: &AgentCatalogAuthority,
    definition: &AgentDefinition,
) -> AgentDefinition {
    AgentRepository::new(store.clone())
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
