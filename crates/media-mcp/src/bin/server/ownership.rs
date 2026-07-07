use std::collections::BTreeSet;

use chrono::{TimeDelta, Utc};
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{
    GatewayInternalIdentity, PlaneCaller, Principal, PrincipalKind, ServerSlug, TokenSubject,
};
use veoveo_media_mcp::state::TaskOwner;

use super::AppState;

const SERVER_SLUG: &str = "media";

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

/// Build a [`PlaneCaller`] for a live, synchronous request: the verified identity
/// plus the raw bearer to forward. Memberships are empty until group `(id, role)`
/// pairs ride in the signed identity (P3).
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
        .map(|b| b.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    Ok(caller_from(identity, bearer))
}

/// Assemble a [`PlaneCaller`] from a verified identity and its raw bearer.
pub(super) fn caller_from(identity: GatewayInternalIdentity, bearer: String) -> PlaneCaller {
    PlaneCaller {
        bearer_token: bearer,
        identity,
        memberships: BTreeSet::new(),
    }
}

/// Build a [`PlaneCaller`] for an **asynchronous** artifact write that completes
/// under a persisted [`TaskOwner`] rather than a live request — media's provider
/// webhook path, where no live gateway bearer exists.
///
/// Media re-mints a short-lived internal token from the stored owner using the
/// shared internal secret it already holds, audienced to `media`. The owner's
/// `principal_id`, `tenant`, and `data_labels` are exactly what the plane's
/// access decision reads; the remaining `Principal` fields are placeholders the
/// plane does not consult. This is media acting on behalf of a principal it
/// already authenticated at task-submit time, for async completion — not a
/// privilege escalation, since the write is attributed to that same owner.
pub(super) fn plane_caller_for_owner(
    state: &AppState,
    owner: &TaskOwner,
) -> Result<PlaneCaller, McpError> {
    let now = Utc::now();
    let principal = Principal {
        id: owner.principal_id.clone(),
        kind: PrincipalKind::Service,
        issuer: state.internal_token_issuer_name.clone(),
        subject: TokenSubject::new("artifact-plane-writer")
            .map_err(|e| McpError::internal_error(e.to_string(), None))?,
        tenant: owner.tenant.clone(),
        groups: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::new(),
        data_labels: owner.data_labels.clone(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(now),
    };
    let server = ServerSlug::new(SERVER_SLUG)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    let issued = state
        .internal_token_issuer
        .issue(
            owner.profile.clone(),
            server,
            principal,
            now + TimeDelta::minutes(5),
        )
        .map_err(|e| {
            McpError::internal_error(format!("minting owner artifact token: {e}"), None)
        })?;
    Ok(PlaneCaller {
        bearer_token: issued.bearer_token,
        identity: issued.identity,
        memberships: BTreeSet::new(),
    })
}

pub(super) fn task_owner_from_identity(
    task_id: &str,
    identity: &GatewayInternalIdentity,
) -> TaskOwner {
    TaskOwner {
        task_id: task_id.to_string(),
        principal_id: identity.principal.id.clone(),
        profile: identity.profile.clone(),
        tenant: identity.principal.tenant.clone(),
        data_labels: identity.principal.data_labels.clone(),
    }
}

pub(super) async fn optional_task_owner(
    state: &AppState,
    task_id: &str,
) -> Result<Option<TaskOwner>, McpError> {
    if let Some(owner) = state.task_owners.read().await.get(task_id).cloned() {
        return Ok(Some(owner));
    }
    let Some(owner) = state
        .durable
        .task_owner(task_id)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?
    else {
        return Ok(None);
    };
    state
        .task_owners
        .write()
        .await
        .insert(task_id.to_string(), owner.clone());
    Ok(Some(owner))
}

pub(super) async fn task_owner(state: &AppState, task_id: &str) -> Result<TaskOwner, McpError> {
    optional_task_owner(state, task_id)
        .await?
        .ok_or_else(|| McpError::invalid_request("task ownership record missing", None))
}

pub(super) async fn require_task_owner(
    state: &AppState,
    context: &RequestContext<RoleServer>,
    task_id: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    let owner = task_owner(state, task_id).await?;
    if task_owner_allows(&owner, &identity) {
        Ok(identity)
    } else {
        Err(McpError::invalid_request(
            "media task policy denied request",
            None,
        ))
    }
}

pub(super) async fn optional_prediction_owner(
    state: &AppState,
    prediction_id: &str,
) -> Result<Option<TaskOwner>, McpError> {
    let Some(task_id) = state
        .durable
        .task_id_for_provider_job_id(prediction_id)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?
    else {
        return Ok(None);
    };
    optional_task_owner(state, &task_id).await
}

pub(super) async fn prediction_owner(
    state: &AppState,
    prediction_id: &str,
) -> Result<TaskOwner, McpError> {
    optional_prediction_owner(state, prediction_id)
        .await?
        .ok_or_else(|| McpError::invalid_request("prediction ownership record missing", None))
}

pub(super) fn task_owner_allows(owner: &TaskOwner, identity: &GatewayInternalIdentity) -> bool {
    owner.principal_id == identity.principal.id
        && owner.profile == identity.profile
        && owner.tenant == identity.principal.tenant
        && owner.data_labels.is_subset(&identity.principal.data_labels)
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
    use veoveo_media_mcp::state::TaskOwner;

    use super::task_owner_allows;

    #[test]
    fn task_owner_requires_principal_profile_and_tenant() {
        let identity = internal_identity_for("default", Some("tenant-a"), &["cui", "pii"]);
        let owner = TaskOwner {
            task_id: "task-1".to_string(),
            principal_id: identity.principal.id.clone(),
            profile: identity.profile.clone(),
            tenant: identity.principal.tenant.clone(),
            data_labels: BTreeSet::from([DataLabelId::new("cui").unwrap()]),
        };

        assert!(task_owner_allows(&owner, &identity));
        assert!(!task_owner_allows(
            &TaskOwner {
                profile: GatewayProfileId::new("research").unwrap(),
                ..owner.clone()
            },
            &identity
        ));
        assert!(!task_owner_allows(
            &TaskOwner {
                tenant: Some(TenantId::new("tenant-b").unwrap()),
                ..owner.clone()
            },
            &identity
        ));
        assert!(!task_owner_allows(
            &TaskOwner {
                data_labels: BTreeSet::from([DataLabelId::new("itar").unwrap()]),
                ..owner
            },
            &identity
        ));
    }

    fn internal_identity_for(
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
