use crate::persistence::AgentRepository;
use std::collections::BTreeSet;
use veoveo_gateway_contract::GatewayAction;
use veoveo_gateway_contract::OAuthClientId;

use crate::contract::authoring as wire;
use crate::persistence::{AgentExecution, instances::ManagedAgentRegistration};
use anyhow::{Context, Result, ensure};
use jsonwebtoken::jwk::{
    AlgorithmParameters, CommonParameters, Jwk, JwkSet, KeyAlgorithm, PublicKeyUse,
    RSAKeyParameters, RSAKeyType,
};
use veoveo_mcp_contract::{
    OAuthClientAuthMethod, OAuthClientRegistration, OAuthClientSurface, OAuthGrantType,
    PolicyTarget, Principal,
};
use veoveo_platform_store::deterministic_principal_id;
use veoveo_types::WorkContextMembershipLevel;
use veoveo_types::{InvocationMode, RoleId, ScopeName, TenantId, WorkContextId};

use super::ManagedTemplateCatalog;
use super::runtime_template_revision;
use crate::contract::ManagedAgentToken;
use futures::future::BoxFuture;
use std::sync::Arc;
use veoveo_mcp_gateway::oauth_clients::{
    EffectiveOAuthClient, OAuthClientAuthority, OAuthClientResolver,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalog, VerifiedAccessToken};
use veoveo_platform_store::PlatformStore;
use veoveo_types::{ExtensionKey, ExtensionRegistry};

/// Reads current durable registration and the installation template on each resolution.
#[derive(Debug, Clone)]
pub struct ManagedOAuthClientResolver {
    platform: PlatformStore,
    templates: Arc<ManagedTemplateCatalog>,
    registry: ExtensionRegistry,
    key: ExtensionKey<ManagedAgentToken>,
}
/// Current managed registration admitted against its installed template and catalog.
#[derive(Debug)]
pub struct AdmittedManagedOAuthClient(ManagedAgentRegistration);
impl AdmittedManagedOAuthClient {
    pub fn into_registration(self) -> ManagedAgentRegistration {
        self.0
    }
}

struct ResolvedClient {
    registration: OAuthClientRegistration,
    public_keys: Option<JwkSet>,
    managed: Option<ManagedAgentRegistration>,
}
impl ResolvedClient {
    fn into_effective(
        self,
        registry: ExtensionRegistry,
        key: ExtensionKey<ManagedAgentToken>,
    ) -> EffectiveOAuthClient {
        match self.managed {
            Some(managed) => EffectiveOAuthClient::contributed(
                self.registration,
                self.public_keys,
                Arc::new(ManagedAuthority {
                    managed,
                    registry,
                    key,
                }),
            ),
            None => EffectiveOAuthClient::installed(self.registration),
        }
    }
}

impl ManagedOAuthClientResolver {
    pub fn new(
        platform: PlatformStore,
        templates: Arc<ManagedTemplateCatalog>,
        registry: ExtensionRegistry,
        key: ExtensionKey<ManagedAgentToken>,
    ) -> Self {
        Self {
            platform,
            templates,
            registry,
            key,
        }
    }
    pub async fn admitted_managed_client(
        &self,
        catalog: &GatewayCatalog,
        id: &OAuthClientId,
    ) -> Result<Option<AdmittedManagedOAuthClient>> {
        Ok(self
            .resolve_current(catalog, id)
            .await?
            .and_then(|client| client.managed.map(AdmittedManagedOAuthClient)))
    }

    async fn resolve_current(
        &self,
        catalog: &GatewayCatalog,
        id: &OAuthClientId,
    ) -> Result<Option<ResolvedClient>> {
        let managed = AgentRepository::new(self.platform.clone())
            .managed_agent_registration(id)
            .await?;
        let installed = catalog.oauth_client(id);
        ensure!(
            managed.is_none() || installed.is_none(),
            "OAuth registration source collision"
        );
        match (installed, managed) {
            (Some(client), None) => Ok(Some(ResolvedClient {
                registration: client.clone(),
                public_keys: None,
                managed: None,
            })),
            (None, Some(managed)) => {
                if !managed.enabled {
                    return Ok(None);
                }
                let AgentExecution::Managed {
                    template,
                    template_revision,
                    ..
                } = &managed.revision.content.execution
                else {
                    anyhow::bail!("managed revision has another execution kind");
                };
                let template = self
                    .templates
                    .get(&wire::AgentTemplateId::parse(template.clone())?)
                    .context("managed template is unavailable")?;
                ensure!(
                    runtime_template_revision(template).as_str()
                        == format!("sha256:{template_revision}"),
                    "managed template changed"
                );
                let instance = &managed.instance;
                let identity = &instance.identity;
                let tenant = TenantId::parse(managed.tenant_key.clone())?;
                let context = WorkContextId::parse(managed.context_key.clone())?;
                let profile = catalog
                    .profile(&template.profile)
                    .context("managed profile is unavailable")?;
                let server = catalog
                    .authorization_server(&profile.authorization_server)
                    .context("managed authorization server is unavailable")?;
                let scopes: BTreeSet<ScopeName> = identity
                    .scopes
                    .iter()
                    .cloned()
                    .map(ScopeName::parse)
                    .collect::<std::result::Result<_, _>>()?;
                ensure!(
                    template.tenant == tenant
                        && template.work_contexts.contains(&context)
                        && identity.profile == template.profile.as_str()
                        && identity.resource == profile.protected_resource.as_str()
                        && identity.authorization_server == profile.authorization_server.as_str()
                        && identity.issuer == server.issuer.as_str()
                        && scopes == template.scopes
                        && identity
                            .roles
                            .iter()
                            .cloned()
                            .map(RoleId::parse)
                            .collect::<std::result::Result<BTreeSet<_>, _>>()?
                            == template.roles
                        && membership(identity.membership) == template.membership,
                    "managed identity exceeds current template authority"
                );
                ensure!(
                    deterministic_principal_id(
                        &managed.tenant_key,
                        &format!("{}#{}", identity.issuer, identity.client_id)
                    )?
                    .record_id()
                        == instance.principal,
                    "managed service principal mismatch"
                );
                let key = instance
                    .public_key
                    .as_ref()
                    .context("managed credential is unavailable")?;
                let public_keys = JwkSet {
                    keys: vec![Jwk {
                        common: CommonParameters {
                            public_key_use: Some(PublicKeyUse::Signature),
                            key_algorithm: Some(KeyAlgorithm::RS256),
                            key_id: Some(key.kid.clone()),
                            ..Default::default()
                        },
                        algorithm: AlgorithmParameters::RSA(RSAKeyParameters {
                            key_type: RSAKeyType::RSA,
                            n: key.n.clone(),
                            e: key.e.clone(),
                        }),
                    }],
                };
                let registration = OAuthClientRegistration {
                    knowledge_indexing: None,
                    id: id.clone(),
                    authorization_server: profile.authorization_server.clone(),
                    default_work_context: context,
                    invocation_mode: InvocationMode::Automated,
                    display_name: Some(instance.name.clone()),
                    client_surface: OAuthClientSurface::FullMcp,
                    allowed_compatibility_helpers: BTreeSet::new(),
                    direct_task_call_adapter: false,
                    allowed_resources: BTreeSet::from([profile.protected_resource.clone()]),
                    grant_types: BTreeSet::from([OAuthGrantType::ClientCredentials]),
                    auth_methods: BTreeSet::from([OAuthClientAuthMethod::PrivateKeyJwt]),
                    redirect_uris: Vec::new(),
                    allowed_scopes: scopes,
                    credential_secret: None,
                    jwks: None,
                    tenant: Some(tenant),
                    metadata: serde_json::Value::Null,
                };
                Ok(Some(ResolvedClient {
                    registration,
                    public_keys: Some(public_keys),
                    managed: Some(managed),
                }))
            }
            _ => Ok(None),
        }
    }
}
impl OAuthClientResolver for ManagedOAuthClientResolver {
    fn resolve<'a>(
        &'a self,
        catalog: &'a GatewayCatalog,
        id: &'a OAuthClientId,
    ) -> BoxFuture<'a, Result<Option<EffectiveOAuthClient>>> {
        Box::pin(async move {
            Ok(self
                .resolve_current(catalog, id)
                .await?
                .map(|client| client.into_effective(self.registry.clone(), self.key.clone())))
        })
    }
}

#[derive(Debug)]
struct ManagedAuthority {
    managed: ManagedAgentRegistration,
    registry: ExtensionRegistry,
    key: ExtensionKey<ManagedAgentToken>,
}
impl OAuthClientAuthority for ManagedAuthority {
    fn membership(
        &self,
        catalog: &GatewayCatalog,
        context: &WorkContextId,
        principal: &Principal,
    ) -> Result<WorkContextMembershipLevel> {
        let managed = &self.managed;
        ensure!(
            context.as_str() == managed.context_key
                && principal
                    .tenant
                    .as_ref()
                    .is_some_and(|tenant| tenant.as_str() == managed.tenant_key),
            "managed Work Context is not admitted"
        );
        let context = catalog
            .work_context(context)
            .context("managed Work Context is unavailable")?;
        ensure!(
            context.tenant.as_str() == managed.tenant_key,
            "managed tenant is not admitted"
        );
        Ok(membership(managed.instance.identity.membership))
    }
    fn token_extensions(&self) -> Result<veoveo_types::AdmittedExtensions> {
        let m = &self.managed;
        let binding = ManagedAgentToken {
            instance: wire::AgentManagedInstanceId::parse(m.instance.key.clone())?,
            generation: m.instance.active_generation,
            epoch: m.instance.dispatch_epoch,
        };
        Ok(self.registry.contribute(&self.key, &binding)?)
    }
    fn execution_attribution(
        &self,
        verified: &VerifiedAccessToken,
    ) -> Result<Option<veoveo_mcp_contract::audit::AuditManagedExecution>> {
        self.validate_token(verified)?;
        let binding = verified
            .extensions
            .get(&self.key)?
            .context("OAuth token registration source mismatch")?;
        Ok(Some(execution_attribution(&binding)?))
    }
    fn apply_service_roles(&self, principal: &mut Principal) -> Result<()> {
        principal.roles = self
            .managed
            .instance
            .identity
            .roles
            .iter()
            .cloned()
            .map(RoleId::parse)
            .collect::<std::result::Result<_, _>>()?;
        Ok(())
    }
    fn validate_token(&self, verified: &VerifiedAccessToken) -> Result<()> {
        let managed = &self.managed;
        let binding = verified
            .extensions
            .get(&self.key)?
            .context("OAuth token registration source mismatch")?;
        ensure!(
            binding.instance.as_str() == managed.instance.key
                && binding.generation == managed.instance.active_generation
                && verified.access_token.issuer.as_str() == managed.instance.identity.issuer
                && verified.access_token.session_family.is_none(),
            "managed token binding is no longer current"
        );
        Ok(())
    }
    fn action_admitted(
        &self,
        catalog: &GatewayCatalog,
        subject: &AuthenticatedSubject,
        action: veoveo_gateway_contract::PolicyAction,
        target: &PolicyTarget,
    ) -> Result<bool> {
        let Some(binding) = subject.extensions.get(&self.key)? else {
            return Ok(false);
        };
        let managed = &self.managed;
        if binding.instance.as_str() != managed.instance.key
            || binding.generation != managed.instance.active_generation
        {
            return Ok(false);
        }
        if action.kernel() == Some(GatewayAction::ToolsCall)
            && binding.epoch != managed.instance.dispatch_epoch
        {
            return Ok(false);
        }
        if matches!(
            action.kernel(),
            Some(GatewayAction::ToolsCall | GatewayAction::ToolsList)
        ) && let PolicyTarget::Tool { server, tool } = target
        {
            let name = catalog.project_tool_name(server, tool)?;
            return Ok(managed
                .revision
                .content
                .tools
                .iter()
                .any(|allowed| allowed == name.as_str()));
        }
        Ok(true)
    }
}

fn membership(
    level: veoveo_platform_store::WorkContextMembershipLevel,
) -> WorkContextMembershipLevel {
    use veoveo_platform_store::WorkContextMembershipLevel as Stored;
    match level {
        Stored::Viewer => WorkContextMembershipLevel::Viewer,
        Stored::Contributor => WorkContextMembershipLevel::Contributor,
        Stored::Custodian => WorkContextMembershipLevel::Custodian,
        Stored::Owner => WorkContextMembershipLevel::Owner,
    }
}

/// Attribution describes the admitted signed execution; it grants no authority.
pub(crate) fn execution_attribution(
    binding: &ManagedAgentToken,
) -> Result<veoveo_mcp_contract::audit::AuditManagedExecution> {
    use std::num::NonZeroU64;
    Ok(veoveo_mcp_contract::audit::AuditManagedExecution {
        instance: binding.instance.clone(),
        generation: NonZeroU64::new(u64::try_from(binding.generation)?)
            .context("managed generation must be positive")?,
        dispatch_epoch: NonZeroU64::new(u64::try_from(binding.epoch)?)
            .context("managed epoch must be positive")?,
        episode: None,
    })
}

#[cfg(test)]
mod attribution_tests {
    use super::*;
    #[test]
    fn attribution_checks_counters_and_preserves_signed_epoch() {
        let mut binding = ManagedAgentToken {
            instance: wire::AgentManagedInstanceId::parse("worker-one").unwrap(),
            generation: i64::MAX,
            epoch: 3,
        };
        let attribution = execution_attribution(&binding).unwrap();
        assert_eq!(
            attribution.generation.get(),
            u64::try_from(i64::MAX).unwrap()
        );
        assert_eq!(attribution.dispatch_epoch.get(), 3);
        for invalid in [i64::MIN, -1, 0] {
            binding.generation = invalid;
            assert!(execution_attribution(&binding).is_err());
            binding.generation = 1;
            binding.epoch = invalid;
            assert!(execution_attribution(&binding).is_err());
        }
    }
}
