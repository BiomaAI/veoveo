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
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{
    GatewayControlPlane, GatewayInternalIdentity, PolicyEffect, PolicyTarget, TraceId,
};
use veoveo_platform_store::{
    GatewayRefreshFamilyRecord, PlatformStore, gateway_jwt_revocation_record_id,
    gateway_refresh_family_record_id,
};
use veoveo_policy::{PolicyCatalog, PolicyCatalogView, PolicyRequest};
use veoveo_types::ScopeDefinition;

pub struct RequestAuthority {
    catalog: PolicyCatalog,
    allowed_tools: Option<std::collections::BTreeSet<veoveo_gateway_contract::GatewayToolName>>,
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
    registry: &veoveo_gateway_contract::CatalogRegistry,
    resolver: &dyn veoveo_policy::internal_clients::InternalClientAuthorityResolver,
    identity: &GatewayInternalIdentity,
    scope: KnowledgeScope,
    action: GatewayAction,
    target: &PolicyTarget,
) -> Result<RequestAuthority, ServiceError> {
    let authority = authenticate(store, registry, resolver, identity, scope).await?;
    if !authority.allows(identity, action, target) {
        return Err(ServiceError::AccessChanged);
    }
    Ok(authority)
}

impl RequestAuthority {
    pub(crate) fn collection_scope(
        &self,
        identity: &GatewayInternalIdentity,
        registration: &CollectionRegistration,
    ) -> Result<veoveo_platform_store::knowledge::CandidateScope, ServiceError> {
        let request = identity
            .request_context
            .as_ref()
            .ok_or(ServiceError::AccessChanged)?;
        let selection = veoveo_policy::admit_resource_reads(
            &self.catalog,
            &request.principal,
            &identity.profile,
            registration.descriptor.collection().server(),
        )
        .map_err(|_| ServiceError::AccessChanged)?;
        let mut scope = self.caller.scope(&Default::default());
        scope.collections = [(registration.descriptor.collection().clone(), selection)].into();
        Ok(scope)
    }
    pub(crate) fn allows(
        &self,
        identity: &GatewayInternalIdentity,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> bool {
        if let (Some(allowed), PolicyTarget::Tool { server, tool }) = (&self.allowed_tools, target)
        {
            let Ok(name) = veoveo_gateway_contract::GatewayToolName::from_parts(server, tool)
            else {
                return false;
            };
            if !allowed.contains(&name) {
                return false;
            }
        }
        let Some(request) = identity.request_context.as_ref() else {
            return false;
        };
        let Ok(trace) = TraceId::parse(&request.audit.trace_id) else {
            return false;
        };
        veoveo_policy::decide(
            &self.catalog,
            PolicyRequest {
                principal: &request.principal,
                profile: &identity.profile,
                action: action.into(),
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
    registry: &veoveo_gateway_contract::CatalogRegistry,
    resolver: &dyn veoveo_policy::internal_clients::InternalClientAuthorityResolver,
    identity: &GatewayInternalIdentity,
    scope: KnowledgeScope,
) -> Result<RequestAuthority, ServiceError> {
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        resolve(store, registry, resolver, identity, scope),
    )
    .await
    .map_err(|_| ServiceError::Deadline)?
}
async fn resolve(
    store: &PlatformStore,
    registry: &veoveo_gateway_contract::CatalogRegistry,
    resolver: &dyn veoveo_policy::internal_clients::InternalClientAuthorityResolver,
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
    let catalog =
        PolicyCatalog::new(plane, registry.clone()).map_err(|_| ServiceError::AccessChanged)?;
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
    let resolved = resolver
        .resolve(
            veoveo_policy::internal_clients::InternalClientAuthorityRequest {
                catalog: &catalog,
                identity,
            },
        )
        .await
        .map_err(|_| ServiceError::AccessChanged)?;
    let memberships = resolved.memberships;
    let allowed_tools = resolved.allowed_tools;
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
