use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};

pub(super) fn internal_identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })
}

pub(super) fn internal_caller(
    context: &RequestContext<RoleServer>,
) -> Result<PlaneCaller, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    let identity = parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    let bearer_token = parts
        .extensions
        .get::<super::auth::ForwardedBearer>()
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    Ok(plane_caller(identity, bearer_token))
}

pub(super) fn plane_caller(identity: GatewayInternalIdentity, bearer_token: String) -> PlaneCaller {
    let memberships = identity.actor.group_memberships();
    PlaneCaller {
        bearer_token,
        identity,
        memberships,
    }
}

pub(super) fn runtime_owner(identity: &GatewayInternalIdentity) -> veoveo_task_runtime::TaskOwner {
    veoveo_task_runtime::TaskOwner {
        principal_key: identity.actor.id.to_string(),
        principal_kind: match identity.actor.kind {
            veoveo_mcp_contract::PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            veoveo_mcp_contract::PrincipalKind::Service => {
                veoveo_task_runtime::PrincipalKind::Service
            }
        },
        issuer: identity.actor.issuer.to_string(),
        subject: identity.actor.subject.to_string(),
        profile: identity.profile.to_string(),
        tenant_key: identity.actor.tenant.as_ref().map(ToString::to_string),
        data_labels: identity
            .actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        authority: identity.authority.clone(),
    }
}

/// Convert authenticated gateway authority into the domain's protocol-independent owner.
pub(super) fn live_view_owner(
    identity: &GatewayInternalIdentity,
) -> crate::contract::LiveViewOwner {
    let authority = &identity.authority;
    let mut data_labels = authority.output_policy.data_labels.clone();
    data_labels.extend(authority.output_policy.classification.clone());
    crate::contract::LiveViewOwner {
        subject: authority.output_policy.owner.clone(),
        tenant: authority.tenant.clone(),
        work_context: authority.work_context.clone(),
        policy_revision: authority.policy_revision.clone(),
        data_labels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use veoveo_types::{AccessSubject, DataLabelId, GroupId};

    #[test]
    fn live_owner_uses_resolved_output_policy_and_combines_classification_labels() {
        let mut identity = crate::server::test_support::identity(
            "tenant",
            "operations",
            "viewer",
            &["actor-only"],
        );
        identity.authority.output_policy.owner =
            AccessSubject::Group(GroupId::new("operators").unwrap());
        identity.authority.output_policy.data_labels =
            BTreeSet::from([DataLabelId::new("sensor").unwrap()]);
        identity.authority.output_policy.classification =
            Some(DataLabelId::new("restricted").unwrap());
        let owner = live_view_owner(&identity);
        assert_eq!(owner.subject, identity.authority.output_policy.owner);
        assert_eq!(owner.tenant, identity.authority.tenant);
        assert_eq!(owner.work_context, identity.authority.work_context);
        assert_eq!(owner.policy_revision, identity.authority.policy_revision);
        assert_eq!(
            owner.data_labels,
            BTreeSet::from([
                DataLabelId::new("sensor").unwrap(),
                DataLabelId::new("restricted").unwrap(),
            ])
        );
        identity.authority.output_policy.classification = Some(DataLabelId::new("sensor").unwrap());
        assert_eq!(live_view_owner(&identity).data_labels.len(), 1);
    }
}
