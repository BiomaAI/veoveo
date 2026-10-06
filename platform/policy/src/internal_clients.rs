//! Current internal-client authority port. Adapters own persistence and freshness.
use crate::{PolicyCatalog, PolicyCatalogView};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
};
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_modules::ObservationTable;
use veoveo_types::{WorkContextId, WorkContextMembershipLevel};

pub struct InternalClientAuthorityRequest<'a> {
    pub catalog: &'a PolicyCatalog,
    pub identity: &'a GatewayInternalIdentity,
}
/// The owner has checked collision, current enabled registration, execution fences,
/// identity profile and delegated scopes/roles/tools before returning these values.
pub struct InternalClientAuthority {
    pub memberships: BTreeMap<WorkContextId, WorkContextMembershipLevel>,
    pub allowed_tools: Option<BTreeSet<GatewayToolName>>,
}
pub type AuthorityFuture<'a> =
    Pin<Box<dyn Future<Output = anyhow::Result<InternalClientAuthority>> + Send + 'a>>;
/// Called again before delivery; no result establishes a cached authority lease.
pub trait InternalClientAuthorityResolver: Send + Sync {
    fn resolve<'a>(&'a self, request: InternalClientAuthorityRequest<'a>) -> AuthorityFuture<'a>;
    fn observation_tables(&self) -> Vec<ObservationTable>;
}
/// For an installation whose admitted effective plan excludes managed clients.
/// It never probes optional-owner tables and rejects any managed attribution.
pub struct StaticInternalClientAuthorityResolver;
impl InternalClientAuthorityResolver for StaticInternalClientAuthorityResolver {
    fn resolve<'a>(&'a self, input: InternalClientAuthorityRequest<'a>) -> AuthorityFuture<'a> {
        Box::pin(async move { static_authority(input) })
    }
    fn observation_tables(&self) -> Vec<ObservationTable> {
        Vec::new()
    }
}
/// Managed adapters may use this only AFTER proving absence of a managed registration,
/// including a static/managed identity collision check.
pub fn static_authority(
    input: InternalClientAuthorityRequest<'_>,
) -> anyhow::Result<InternalClientAuthority> {
    let identity = input.identity;
    let control = input.catalog.control_plane();
    let request = identity
        .request_context
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing authenticated request context"))?;
    let token = &request.access_token;
    anyhow::ensure!(
        token.managed_execution.is_none(),
        "managed attribution requires its selected owner adapter"
    );
    let profile = input
        .catalog
        .profile(&identity.profile)
        .ok_or_else(|| anyhow::anyhow!("unknown internal client profile"))?;
    let authorization_server = control
        .authorization_servers
        .iter()
        .find(|server| server.id == profile.authorization_server)
        .ok_or_else(|| anyhow::anyhow!("unknown internal authorization server"))?;
    anyhow::ensure!(
        token.issuer == authorization_server.issuer && token.audience == profile.protected_resource,
        "internal token profile changed"
    );
    let client = control
        .oauth_clients
        .iter()
        .find(|client| client.id == token.oauth_client_id)
        .ok_or_else(|| anyhow::anyhow!("unregistered internal client"))?;
    let memberships = {
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
            anyhow::bail!("internal client authority changed");
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
    };
    Ok(InternalClientAuthority {
        memberships,
        allowed_tools: None,
    })
}
