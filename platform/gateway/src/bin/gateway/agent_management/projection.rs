use std::collections::BTreeMap;
use surrealdb::types::ToSql;

use veoveo_mcp_contract::{Sha256Digest, WorkContextId, agent_management as wire};
use veoveo_mcp_gateway::GatewayCatalog;
use veoveo_platform_store::{
    RecordId, RecordIdKey, agent_management as domain, deterministic_work_context_id,
};

use super::{AgentManagementState, Fault, authority::Admission};

pub(super) fn uuid(id: &RecordId) -> Result<uuid::Uuid, Fault> {
    match &id.key {
        RecordIdKey::Uuid(id) => Ok(**id),
        _ => Err(Fault::unavailable()),
    }
}

pub(super) fn content(value: wire::Content) -> domain::AgentContent {
    domain::AgentContent {
        model: domain::AgentModelReference {
            id: value.model.id.to_string(),
            revision: value.model.revision.hex().to_owned(),
        },
        instructions: value.instructions,
        tools: value.tools.into_iter().map(String::from).collect(),
        budgets: domain::AgentBudgets {
            max_output_tokens: value.budgets.max_output_tokens,
            max_completion_calls: value.budgets.max_completion_calls,
            max_tool_calls: value.budgets.max_tool_calls,
            deadline_seconds: value.budgets.deadline_seconds,
        },
        execution: match value.execution {
            wire::Execution::Chat => domain::AgentExecution::Chat,
            wire::Execution::Managed {
                template,
                template_revision,
                parameters,
                resource_subscriptions,
            } => domain::AgentExecution::Managed {
                template: template.to_string(),
                template_revision: template_revision.hex().to_owned(),
                parameters: parameters
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            match v {
                                wire::TemplateParameter::Text(v) => {
                                    domain::AgentTemplateParameter::Text(v)
                                }
                                wire::TemplateParameter::Integer(v) => {
                                    domain::AgentTemplateParameter::Integer(v)
                                }
                                wire::TemplateParameter::Boolean(v) => {
                                    domain::AgentTemplateParameter::Boolean(v)
                                }
                            },
                        )
                    })
                    .collect(),
                resource_subscriptions: resource_subscriptions
                    .into_iter()
                    .map(String::from)
                    .collect(),
            },
        },
    }
}

pub(super) fn public_content(value: domain::AgentContent) -> Result<wire::Content, Fault> {
    Ok(wire::Content {
        model: wire::ModelReference {
            id: wire::AgentModelId::new(value.model.id).map_err(|_| Fault::unavailable())?,
            revision: digest(&value.model.revision)?,
        },
        instructions: value.instructions,
        tools: value
            .tools
            .into_iter()
            .map(|t| veoveo_mcp_contract::GatewayToolName::new(t).map_err(|_| Fault::unavailable()))
            .collect::<Result<_, _>>()?,
        budgets: wire::Budgets {
            max_output_tokens: value.budgets.max_output_tokens,
            max_completion_calls: value.budgets.max_completion_calls,
            max_tool_calls: value.budgets.max_tool_calls,
            deadline_seconds: value.budgets.deadline_seconds,
        },
        execution: match value.execution {
            domain::AgentExecution::Chat => wire::Execution::Chat,
            domain::AgentExecution::Managed {
                template,
                template_revision,
                parameters,
                resource_subscriptions,
            } => wire::Execution::Managed {
                template: wire::AgentTemplateId::new(template).map_err(|_| Fault::unavailable())?,
                template_revision: digest(&template_revision)?,
                parameters: parameters
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            match v {
                                domain::AgentTemplateParameter::Text(v) => {
                                    wire::TemplateParameter::Text(v)
                                }
                                domain::AgentTemplateParameter::Integer(v) => {
                                    wire::TemplateParameter::Integer(v)
                                }
                                domain::AgentTemplateParameter::Boolean(v) => {
                                    wire::TemplateParameter::Boolean(v)
                                }
                            },
                        )
                    })
                    .collect(),
                resource_subscriptions: resource_subscriptions
                    .into_iter()
                    .map(|u| {
                        veoveo_mcp_contract::ResourceUri::new(u).map_err(|_| Fault::unavailable())
                    })
                    .collect::<Result<_, _>>()?,
            },
        },
    })
}

pub(super) fn digest(value: &str) -> Result<Sha256Digest, Fault> {
    Sha256Digest::from_hex(value).map_err(|_| Fault::unavailable())
}

pub(super) fn context(
    catalog: &GatewayCatalog,
    tenant: &veoveo_mcp_contract::TenantId,
    id: &RecordId,
) -> Result<WorkContextId, Fault> {
    catalog
        .control_plane()
        .work_contexts
        .iter()
        .find(|c| {
            c.tenant == *tenant
                && deterministic_work_context_id(tenant.as_str(), c.id.as_str())
                    .is_ok_and(|v| v.record_id() == *id)
        })
        .map(|c| c.id.clone())
        .ok_or_else(Fault::unavailable)
}

pub(super) async fn definitions(
    state: &AgentManagementState,
    admitted: &Admission,
    definitions: Vec<domain::AgentDefinition>,
) -> Result<Vec<wire::Definition>, Fault> {
    let heads: Vec<RecordId> = definitions
        .iter()
        .filter_map(|d| d.published.clone())
        .collect();
    // One bounded batch avoids a separate database request per catalog item.
    let mut response = state
        .store()
        .client()
        .query("SELECT * FROM agent_definition_revision WHERE id IN $heads LIMIT 200;")
        .bind(("heads", heads))
        .await
        .map_err(|_| Fault::unavailable())?
        .check()
        .map_err(|_| Fault::unavailable())?;
    let revisions: Vec<domain::AgentRevision> =
        response.take(0).map_err(|_| Fault::unavailable())?;
    let revisions: BTreeMap<_, _> = revisions
        .into_iter()
        .map(|r| (r.id.to_sql(), r.digest))
        .collect();
    definitions
        .into_iter()
        .map(|d| {
            Ok(wire::Definition {
                id: wire::AgentDefinitionId::new(d.key).map_err(|_| Fault::unavailable())?,
                name: d.name,
                description: d.description,
                owner: uuid(&d.owner)?,
                work_context: context(
                    &admitted.catalog,
                    &admitted.subject.authority.tenant,
                    &d.work_context,
                )?,
                revision: d.revision,
                status: match d.status {
                    domain::AgentDefinitionStatus::Enabled => wire::DefinitionStatus::Enabled,
                    domain::AgentDefinitionStatus::Disabled => wire::DefinitionStatus::Disabled,
                    domain::AgentDefinitionStatus::Archived => wire::DefinitionStatus::Archived,
                },
                disabled: d.disabled,
                draft_digest: digest(&d.draft_digest)?,
                published_digest: d
                    .published
                    .map(|id| {
                        revisions
                            .get(&id.to_sql())
                            .ok_or_else(Fault::unavailable)
                            .and_then(|v| digest(v))
                    })
                    .transpose()?,
                audience: d
                    .audience
                    .iter()
                    .map(|id| context(&admitted.catalog, &admitted.subject.authority.tenant, id))
                    .collect::<Result<_, _>>()?,
                updated_at: d.updated_at,
            })
        })
        .collect()
}
