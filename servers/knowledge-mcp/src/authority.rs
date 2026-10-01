//! Request-owned policy snapshots. The HTTP adapter verifies the internal signature;
//! this adapter checks its lifetime and current registration before and after work.
use crate::{
    ServiceError,
    access::SearchCaller,
    contract::{CollectionRegistration, KnowledgeScope},
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use veoveo_mcp_contract::{
    GatewayAction, GatewayControlPlane, GatewayInternalIdentity, PolicyEffect, PolicyTarget,
    TraceId,
};
use veoveo_platform_store::{
    GatewayRefreshFamilyRecord, PlatformStore, gateway_jwt_revocation_record_id,
    gateway_refresh_family_record_id,
};
use veoveo_policy::{PolicyCatalog, PolicyCatalogView, PolicyRequest};
use veoveo_types::{ScopeDefinition, WorkContextId, WorkContextMembershipLevel};

pub struct RequestAuthority {
    catalog: PolicyCatalog,
    allowed_tools: Option<std::collections::BTreeSet<veoveo_mcp_contract::GatewayToolName>>,
    pub(crate) control_digest: String,
    pub(crate) collections: Vec<CollectionRegistration>,
    pub(crate) approvals: BTreeMap<
        veoveo_mcp_knowledge_extension::CollectionId,
        crate::contract::KnowledgeCollectionApproval,
    >,
    pub(crate) caller: SearchCaller,
}

pub async fn authorize(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    scope: KnowledgeScope,
    action: GatewayAction,
    target: &PolicyTarget,
) -> Result<RequestAuthority, ServiceError> {
    let authority = authenticate(store, identity, scope).await?;
    if !authority.allows(identity, action, target) {
        return Err(ServiceError::AccessChanged);
    }
    Ok(authority)
}

impl RequestAuthority {
    pub(crate) fn allows(
        &self,
        identity: &GatewayInternalIdentity,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> bool {
        if let (Some(allowed), PolicyTarget::Tool { server, tool }) = (&self.allowed_tools, target)
        {
            let Ok(name) = veoveo_mcp_contract::GatewayToolName::from_parts(server, tool) else {
                return false;
            };
            if !allowed.contains(&name) {
                return false;
            }
        }
        let Some(request) = identity.request_context.as_ref() else {
            return false;
        };
        let Ok(trace) = TraceId::new(request.audit.trace_id.to_string()) else {
            return false;
        };
        veoveo_policy::decide(
            &self.catalog,
            PolicyRequest {
                principal: &request.principal,
                profile: &identity.profile,
                action,
                target,
                trace_id: &trace,
            },
        )
        .effect
            == PolicyEffect::Allow
    }
}
pub(crate) async fn authenticate(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    scope: KnowledgeScope,
) -> Result<RequestAuthority, ServiceError> {
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        resolve(store, identity, scope),
    )
    .await
    .map_err(|_| ServiceError::Deadline)?
}
async fn resolve(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    scope: KnowledgeScope,
) -> Result<RequestAuthority, ServiceError> {
    check_lifetime(identity)?;
    let request = identity
        .request_context
        .as_ref()
        .ok_or(ServiceError::AccessChanged)?;
    request
        .validate_for(&identity.actor, &identity.authority)
        .map_err(|_| ServiceError::AccessChanged)?;
    if !identity.actor.scopes.contains(scope.name()) {
        return Err(ServiceError::AccessChanged);
    }
    for principal in [&request.principal, &identity.actor] {
        if !store
            .identity_enabled(veoveo_platform_store::PrincipalIdentityRef {
                tenant: &identity.authority.tenant,
                principal: &principal.id,
                issuer: &principal.issuer,
                subject: &principal.subject,
                kind: match principal.kind {
                    veoveo_mcp_contract::PrincipalKind::User => {
                        veoveo_platform_store::PrincipalKind::User
                    }
                    veoveo_mcp_contract::PrincipalKind::Service => {
                        veoveo_platform_store::PrincipalKind::Service
                    }
                },
            })
            .await?
        {
            return Err(ServiceError::AccessChanged);
        }
    }
    let revision = store
        .active_gateway_control_revision()
        .await?
        .ok_or(ServiceError::AccessChanged)?;
    let plane: GatewayControlPlane = serde_json::from_value(
        serde_json::to_value(revision.control_plane).map_err(|_| ServiceError::AccessChanged)?,
    )
    .map_err(|_| ServiceError::AccessChanged)?;
    let digest = veoveo_types::Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&plane).map_err(|_| ServiceError::AccessChanged)?).into(),
    )
    .hex()
    .to_owned();
    if digest != revision.sha256 {
        return Err(ServiceError::AccessChanged);
    }
    let catalog = PolicyCatalog::new(plane).map_err(|_| ServiceError::AccessChanged)?;
    let profile = catalog
        .profile(&identity.profile)
        .ok_or(ServiceError::AccessChanged)?;
    let control = catalog.control_plane();
    let authorization_server = control
        .authorization_servers
        .iter()
        .find(|server| server.id == profile.authorization_server)
        .ok_or(ServiceError::AccessChanged)?;
    let token = &request.access_token;
    if token.issuer != authorization_server.issuer || token.audience != profile.protected_resource {
        return Err(ServiceError::AccessChanged);
    }
    let installed = control
        .oauth_clients
        .iter()
        .find(|client| client.id == token.oauth_client_id);
    let managed = store
        .managed_agent_registration(token.oauth_client_id.as_str())
        .await
        .map_err(|_| ServiceError::AccessChanged)?;
    let mut allowed_tools = None;
    let memberships: BTreeMap<WorkContextId, WorkContextMembershipLevel> =
        match (installed, managed, &token.managed_agent) {
            (Some(client), None, None) => {
                if client.authorization_server != profile.authorization_server
                    || client.invocation_mode != token.invocation_mode
                    || !client
                        .allowed_resources
                        .contains(&profile.protected_resource)
                    || !token.scopes.is_subset(&client.allowed_scopes)
                    || client
                        .tenant
                        .as_ref()
                        .is_some_and(|tenant| tenant != &identity.authority.tenant)
                {
                    return Err(ServiceError::AccessChanged);
                }
                control
                    .work_contexts
                    .iter()
                    .filter(|context| context.tenant == identity.authority.tenant)
                    .filter_map(|context| {
                        context
                            .membership_for(&request.principal, &client.id)
                            .map(|level| (context.id.clone(), level))
                    })
                    .collect()
            }
            (None, Some(managed), Some(binding)) => {
                let current = &managed.instance;
                if !managed.enabled
                    || managed.tenant_key != identity.authority.tenant.as_str()
                    || managed.context_key != identity.authority.work_context.as_str()
                    || binding.instance.as_str() != current.key
                    || binding.generation != current.active_generation
                    || binding.epoch != current.dispatch_epoch
                    || current.identity.profile != identity.profile.as_str()
                    || current.identity.issuer != token.issuer.as_str()
                    || current.identity.resource != token.audience.as_str()
                    || current.identity.authorization_server
                        != profile.authorization_server.as_str()
                    || token.session_family.is_some()
                    || token
                        .scopes
                        .iter()
                        .any(|scope| !current.identity.scopes.iter().any(|s| s == scope.as_str()))
                    || request
                        .principal
                        .roles
                        .iter()
                        .map(ToString::to_string)
                        .collect::<std::collections::BTreeSet<_>>()
                        != current.identity.roles.iter().cloned().collect()
                {
                    return Err(ServiceError::AccessChanged);
                }
                allowed_tools = Some(
                    managed
                        .revision
                        .content
                        .tools
                        .iter()
                        .map(|name| name.parse())
                        .collect::<Result<_, _>>()
                        .map_err(|_| ServiceError::AccessChanged)?,
                );
                use veoveo_platform_store::WorkContextMembershipLevel as Stored;
                let level = match current.identity.membership {
                    Stored::Viewer => WorkContextMembershipLevel::Viewer,
                    Stored::Contributor => WorkContextMembershipLevel::Contributor,
                    Stored::Custodian => WorkContextMembershipLevel::Custodian,
                    Stored::Owner => WorkContextMembershipLevel::Owner,
                };
                [(identity.authority.work_context.clone(), level)].into()
            }
            _ => return Err(ServiceError::AccessChanged),
        };
    if let Some(family) = &token.session_family {
        let id = gateway_refresh_family_record_id(
            family
                .as_str()
                .parse()
                .map_err(|_| ServiceError::AccessChanged)?,
        );
        let stored: Option<GatewayRefreshFamilyRecord> = store
            .client()
            .select(id.clone())
            .await
            .map_err(veoveo_platform_store::StoreError::from)?;
        let stored = stored
            .filter(|stored| stored.id == id)
            .ok_or(ServiceError::AccessChanged)?;
        if !veoveo_policy::session::SessionFamilyAuthority::from_stored(&stored)
            .map_err(|_| ServiceError::AccessChanged)?
            .allows(veoveo_policy::session::SessionRequest {
                profile: &identity.profile,
                authorization_server: &profile.authorization_server,
                token,
                principal: &request.principal,
                now: Utc::now(),
            })
        {
            return Err(ServiceError::AccessChanged);
        }
    }
    if let Some(jwt) = &token.jwt_id
        && store
            .gateway_jwt_revocation(
                gateway_jwt_revocation_record_id(
                    identity.profile.as_str(),
                    token.issuer.as_str(),
                    jwt.as_str(),
                ),
                Utc::now(),
            )
            .await?
            .is_some()
    {
        return Err(ServiceError::AccessChanged);
    }
    let approvals = control
        .servers
        .iter()
        .filter(|server| {
            veoveo_policy::admit_resource_reads(
                &catalog,
                &request.principal,
                &identity.profile,
                &server.slug,
            )
            .is_ok()
        })
        .flat_map(|server| {
            server
                .knowledge
                .iter()
                .map(|approval| (approval.collection.clone(), approval.clone()))
        })
        .collect();
    let collections = if scope == KnowledgeScope::Search {
        store
            .readable_knowledge_collections(
                &identity.authority.tenant,
                &approvals,
                &identity.actor.scopes,
                veoveo_platform_store::knowledge::CatalogSelection::All,
            )
            .await?
    } else {
        Vec::new()
    };
    let caller = SearchCaller::from_current_memberships(
        &catalog,
        &request.principal,
        &identity.actor,
        &identity.profile,
        &identity.authority,
        &memberships,
        collections.iter().map(|r| &r.descriptor),
    )?;
    check_lifetime(identity)?;
    Ok(RequestAuthority {
        catalog,
        allowed_tools,
        control_digest: digest,
        collections,
        approvals,
        caller,
    })
}
fn check_lifetime(identity: &GatewayInternalIdentity) -> Result<(), ServiceError> {
    let now = Utc::now();
    if identity.server.as_str() != "knowledge"
        || identity.not_before > now
        || identity.expires_at <= now
        || identity.request_context.as_ref().is_none_or(|request| {
            request.access_token.expires_at <= now
                || request.access_token.not_before.is_some_and(|at| at > now)
                || request.access_token.issued_at > now
        })
    {
        return Err(ServiceError::AccessChanged);
    }
    Ok(())
}
