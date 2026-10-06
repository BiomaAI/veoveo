//! Agent-owned live internal-client authority and revocation observation.
use crate::persistence::AgentRepository;
use veoveo_platform_store::PlatformStore;
use veoveo_policy::{
    PolicyCatalogView,
    internal_clients::{
        AuthorityFuture, InternalClientAuthority, InternalClientAuthorityRequest,
        InternalClientAuthorityResolver, static_authority,
    },
};
use veoveo_types::WorkContextMembershipLevel;

pub struct ManagedInternalClientAuthorityResolver {
    repository: AgentRepository,
}
impl ManagedInternalClientAuthorityResolver {
    pub fn new(store: PlatformStore) -> Self {
        Self {
            repository: AgentRepository::new(store),
        }
    }
}
impl InternalClientAuthorityResolver for ManagedInternalClientAuthorityResolver {
    fn resolve<'a>(&'a self, input: InternalClientAuthorityRequest<'a>) -> AuthorityFuture<'a> {
        Box::pin(async move {
            let identity = input.identity;
            let request = identity
                .request_context
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing authenticated request context"))?;
            let token = &request.access_token;
            let control = input.catalog.control_plane();
            let profile = input
                .catalog
                .profile(&identity.profile)
                .ok_or_else(|| anyhow::anyhow!("unknown internal client profile"))?;
            let installed = control
                .oauth_clients
                .iter()
                .find(|client| client.id == token.oauth_client_id);
            // Always query the managed owner before selecting a branch: static and
            // managed registrations with the same client identity are a hard denial.
            let managed = self
                .repository
                .managed_agent_registration(&token.oauth_client_id)
                .await?;
            match (installed, managed, &token.managed_execution) {
                (Some(_), None, None) => static_authority(input),
                (None, Some(managed), Some(binding)) => {
                    let current = &managed.instance;
                    if !managed.enabled
                        || managed.tenant_key != identity.authority.tenant.as_str()
                        || managed.context_key != identity.authority.work_context.as_str()
                        || binding.instance.as_str() != current.key
                        || Some(binding.generation)
                            != checked_execution_counter(current.active_generation)
                        || Some(binding.dispatch_epoch)
                            != checked_execution_counter(current.dispatch_epoch)
                        || current.identity.profile != identity.profile.as_str()
                        || current.identity.issuer != token.issuer.as_str()
                        || current.identity.resource != token.audience.as_str()
                        || current.identity.authorization_server
                            != profile.authorization_server.as_str()
                        || token.session_family.is_some()
                        || token.scopes.iter().any(|scope| {
                            !current.identity.scopes.iter().any(|s| s == scope.as_str())
                        })
                        || request
                            .principal
                            .roles
                            .iter()
                            .map(ToString::to_string)
                            .collect::<std::collections::BTreeSet<_>>()
                            != current.identity.roles.iter().cloned().collect()
                    {
                        anyhow::bail!("managed internal client authority changed");
                    }
                    let allowed_tools = Some(
                        managed
                            .revision
                            .content
                            .tools
                            .iter()
                            .map(|name| name.parse())
                            .collect::<Result<_, _>>()
                            .map_err(|_| anyhow::anyhow!("invalid managed tool authority"))?,
                    );
                    use veoveo_platform_store::WorkContextMembershipLevel as Stored;
                    let level = match current.identity.membership {
                        Stored::Viewer => WorkContextMembershipLevel::Viewer,
                        Stored::Contributor => WorkContextMembershipLevel::Contributor,
                        Stored::Custodian => WorkContextMembershipLevel::Custodian,
                        Stored::Owner => WorkContextMembershipLevel::Owner,
                    };
                    let memberships = [(identity.authority.work_context.clone(), level)].into();
                    Ok(InternalClientAuthority {
                        memberships,
                        allowed_tools,
                    })
                }
                _ => anyhow::bail!("internal client collision or attribution mismatch"),
            }
        })
    }
    fn observation_tables(&self) -> Vec<veoveo_modules::ObservationTable> {
        vec![
            crate::AgentObservationTable::ManagedAgent.into(),
            crate::AgentObservationTable::AgentDefinition.into(),
        ]
    }
}
fn checked_execution_counter(value: i64) -> Option<std::num::NonZeroU64> {
    u64::try_from(value)
        .ok()
        .and_then(std::num::NonZeroU64::new)
}
