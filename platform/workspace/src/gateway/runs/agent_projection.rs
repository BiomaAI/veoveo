//! Workspace presentation and persisted chat bindings use admitted Agent facts.
use veoveo_agent_runtime::gateway::http::execution::{
    ExecutableAgentCatalog, PublishedAgentRevision, ResolvedAgent,
};
use veoveo_mcp_contract::workspace as wire;
use veoveo_platform_store::workspace::WorkspaceAgentAdmission;
pub(super) fn admission(agent: &ResolvedAgent) -> WorkspaceAgentAdmission {
    WorkspaceAgentAdmission {
        definition: agent.id.to_string(),
        definition_digest: agent.revision.hex().to_owned(),
        display_name: agent.name.clone(),
        provider: agent.model.provider.clone(),
        model: agent.model.model.clone(),
    }
}
pub(super) fn revision(value: PublishedAgentRevision) -> wire::AgentRevisionView {
    wire::AgentRevisionView {
        revision: value.revision,
        model: value.model,
        tools: value.tools,
        budgets: value.budgets,
        instructions_digest: value.instructions_digest,
        instructions: value.instructions,
        published_by: wire::PersonId(value.published_by.as_uuid()),
        published_at: value.published_at,
        published_by_name: value.published_by_name,
    }
}
pub(super) fn catalog(value: ExecutableAgentCatalog) -> wire::AgentCatalogPage {
    wire::AgentCatalogPage {
        items: value
            .items
            .into_iter()
            .map(|v| wire::AgentDefinition {
                id: v.id.to_string(),
                name: v.name,
                description: v.description,
                provider: v.provider,
                model: v.model,
                revision: v.revision,
                tools: v.tools,
            })
            .collect(),
        next: value.next.map(|v| v.to_string()),
    }
}
