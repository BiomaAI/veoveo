use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_task_runtime::TaskOwner;

use super::AppState;

pub(super) fn internal_identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))?;
    parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))
}

pub(super) fn internal_caller(
    context: &RequestContext<RoleServer>,
) -> Result<PlaneCaller, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))?;
    let identity = parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))?;
    let bearer = parts
        .extensions
        .get::<super::internal_auth::ForwardedBearer>()
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    Ok(caller_from(identity, bearer))
}

pub(super) fn caller_from(identity: GatewayInternalIdentity, bearer: String) -> PlaneCaller {
    let memberships = identity.principal.group_memberships();
    PlaneCaller {
        bearer_token: bearer,
        identity,
        memberships,
    }
}

pub(super) fn runtime_owner(identity: &GatewayInternalIdentity) -> TaskOwner {
    TaskOwner {
        principal_key: identity.principal.id.as_str().to_owned(),
        principal_kind: match identity.principal.kind {
            veoveo_mcp_contract::PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            veoveo_mcp_contract::PrincipalKind::Service => {
                veoveo_task_runtime::PrincipalKind::Service
            }
        },
        issuer: identity.principal.issuer.as_str().to_owned(),
        subject: identity.principal.subject.as_str().to_owned(),
        profile: identity.profile.as_str().to_owned(),
        tenant_key: identity
            .principal
            .tenant
            .as_ref()
            .map(|tenant| tenant.as_str().to_owned()),
        data_labels: identity
            .principal
            .data_labels
            .iter()
            .map(|label| label.as_str().to_owned())
            .collect(),
    }
}

pub(super) async fn optional_task_owner(
    state: &AppState,
    task_id: &str,
) -> Result<Option<TaskOwner>, McpError> {
    state
        .tasks
        .owner(task_id)
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))
}

pub(super) async fn require_task_owner(
    state: &AppState,
    context: &RequestContext<RoleServer>,
    task_id: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    let owner = optional_task_owner(state, task_id)
        .await?
        .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
    if task_owner_allows(&owner, &identity) {
        Ok(identity)
    } else {
        Err(McpError::invalid_params("unknown task id", None))
    }
}

pub(super) async fn optional_prediction_owner(
    state: &AppState,
    prediction_id: &str,
) -> Result<Option<TaskOwner>, McpError> {
    let Some(job) = state
        .durable
        .provider_job_for_external(prediction_id)
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?
    else {
        return Ok(None);
    };
    optional_task_owner(state, &job.task_id.to_string()).await
}

pub(super) async fn prediction_owner(
    state: &AppState,
    prediction_id: &str,
) -> Result<TaskOwner, McpError> {
    optional_prediction_owner(state, prediction_id)
        .await?
        .ok_or_else(|| McpError::invalid_params("unknown prediction", None))
}

pub(super) fn task_owner_allows(owner: &TaskOwner, identity: &GatewayInternalIdentity) -> bool {
    let caller = runtime_owner(identity);
    owner.allows(
        &caller.principal_key,
        &caller.profile,
        caller.tenant_key.as_deref(),
        &caller.data_labels,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::Utc;
    use veoveo_mcp_contract::{
        DataLabelId, GatewayInternalIdentity, GatewayProfileId, GroupId, JwtId, Principal,
        PrincipalId, PrincipalKind, RoleId, ScopeName, ServerSlug, TenantId, TokenIssuer,
        TokenSubject,
    };

    use super::{runtime_owner, task_owner_allows};

    #[test]
    fn task_owner_requires_principal_profile_tenant_and_labels() {
        let identity = identity("default", Some("tenant-a"), &["cui", "pii"]);
        let owner = runtime_owner(&identity);
        assert!(task_owner_allows(&owner, &identity));

        let mut wrong_profile = owner.clone();
        wrong_profile.profile = "research".into();
        assert!(!task_owner_allows(&wrong_profile, &identity));

        let mut too_sensitive = owner;
        too_sensitive.data_labels.insert("itar".into());
        assert!(!task_owner_allows(&too_sensitive, &identity));
    }

    fn identity(
        profile: &str,
        tenant: Option<&str>,
        data_labels: &[&str],
    ) -> GatewayInternalIdentity {
        let issuer = TokenIssuer::new("https://idp.example.com").unwrap();
        let subject = TokenSubject::new("user-1").unwrap();
        let principal = Principal {
            id: PrincipalId::new(format!("{issuer}#{subject}")).unwrap(),
            kind: PrincipalKind::User,
            issuer,
            subject,
            tenant: tenant.map(TenantId::new).transpose().unwrap(),
            groups: BTreeSet::<GroupId>::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::<RoleId>::new(),
            scopes: BTreeSet::<ScopeName>::new(),
            data_labels: data_labels
                .iter()
                .map(|label| DataLabelId::new(*label).unwrap())
                .collect(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(Utc::now()),
        };
        let now = Utc::now();
        GatewayInternalIdentity {
            issuer: TokenIssuer::new("veoveo-internal").unwrap(),
            profile: GatewayProfileId::new(profile).unwrap(),
            server: ServerSlug::new("media").unwrap(),
            principal,
            jwt_id: JwtId::new("test-jwt").unwrap(),
            issued_at: now,
            not_before: now,
            expires_at: now + chrono::TimeDelta::minutes(5),
        }
    }
}
