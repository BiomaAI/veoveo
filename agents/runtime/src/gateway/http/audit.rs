use crate::contract::AgentAction;
use veoveo_mcp_contract::audit::{AdministrativeOperation, AuditTarget};
pub(super) fn managed_instance_audit_target(
    tenant: &veoveo_types::TenantId,
    instance: &veoveo_types::AgentManagedInstanceId,
) -> AuditTarget {
    use veoveo_types::{ResourceUriBuilder, UriSegment};
    AuditTarget::PlatformResource {
        uri: ResourceUriBuilder::new("veoveo://agent-instances")
            .expect("declared route")
            .segment(UriSegment::new(instance.to_string()).expect("checked instance"))
            .query_pair("tenant", tenant.as_str())
            .expect("checked tenant")
            .build()
            .expect("typed instance address"),
    }
}

pub(super) fn agent_definition_audit_target(
    tenant: &veoveo_types::TenantId,
    definition: &veoveo_types::AgentDefinitionId,
) -> AuditTarget {
    use veoveo_types::{ResourceUriBuilder, UriSegment};
    AuditTarget::PlatformResource {
        uri: ResourceUriBuilder::new("veoveo://agent-definitions")
            .expect("declared route")
            .segment(UriSegment::new(definition.to_string()).expect("checked definition"))
            .query_pair("tenant", tenant.as_str())
            .expect("checked tenant")
            .build()
            .expect("typed definition address"),
    }
}

pub(super) fn agent_management_operation(
    action: AgentAction,
) -> anyhow::Result<AdministrativeOperation> {
    Ok(match action {
        AgentAction::AgentDefinitionsRead => AdministrativeOperation::AgentDefinitionsRead,
        AgentAction::AgentDefinitionsReadContent => {
            AdministrativeOperation::AgentDefinitionsReadContent
        }
        AgentAction::AgentDefinitionsCreate => AdministrativeOperation::AgentDefinitionsCreate,
        AgentAction::AgentDefinitionsEdit => AdministrativeOperation::AgentDefinitionsEdit,
        AgentAction::AgentDefinitionsPublish => AdministrativeOperation::AgentDefinitionsPublish,
        AgentAction::AgentDefinitionsUse => AdministrativeOperation::AgentDefinitionsUse,
        AgentAction::AgentDefinitionsControl => AdministrativeOperation::AgentDefinitionsControl,
        AgentAction::AgentDefinitionsArchive => AdministrativeOperation::AgentDefinitionsArchive,
        AgentAction::AgentDefinitionsTransfer => AdministrativeOperation::AgentDefinitionsTransfer,
        AgentAction::AgentInstancesDeploy => AdministrativeOperation::AgentInstancesDeploy,
        AgentAction::AgentInstancesControl => AdministrativeOperation::AgentInstancesControl,
        _ => anyhow::bail!("action does not belong to agent management"),
    })
}
