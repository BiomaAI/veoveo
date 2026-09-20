use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use jsonwebtoken::jwk::{
    AlgorithmParameters, CommonParameters, Jwk, JwkSet, KeyAlgorithm, PublicKeyUse,
    RSAKeyParameters, RSAKeyType,
};
use veoveo_mcp_contract::{
    GatewayAction, InvocationMode, OAuthClientAuthMethod, OAuthClientId, OAuthClientRegistration,
    OAuthClientSurface, OAuthGrantType, PolicyTarget, Principal, RoleId, ScopeName, TenantId,
    WorkContextId, WorkContextMembershipLevel, agent_management as wire,
};
use veoveo_platform_store::{
    agent_management::{AgentExecution, instances::ManagedAgentRegistration},
    deterministic_principal_id,
};

use super::runtime_template_revision;
use crate::{AuthenticatedSubject, GatewayCatalog, GatewayState, VerifiedAccessToken};

/// One source-owned effective client. Public verification material is separate
/// from installation JWKS references; there is no invented file or network URL.
#[derive(Clone, Debug)]
pub struct EffectiveOAuthClient {
    pub registration: OAuthClientRegistration,
    pub public_keys: Option<JwkSet>,
    pub managed: Option<ManagedAgentRegistration>,
}

impl EffectiveOAuthClient {
    pub fn membership(
        &self,
        catalog: &GatewayCatalog,
        context: &WorkContextId,
        principal: &Principal,
    ) -> Result<WorkContextMembershipLevel> {
        let Some(managed) = &self.managed else {
            return Ok(catalog.work_context_membership(
                &self.registration.id,
                context,
                principal,
            )?);
        };
        ensure!(
            context.as_str() == managed.context_key
                && principal.tenant.as_ref() == self.registration.tenant.as_ref(),
            "managed Work Context is not admitted"
        );
        let context = catalog
            .work_context(context)
            .context("managed Work Context is unavailable")?;
        ensure!(
            Some(&context.tenant) == self.registration.tenant.as_ref(),
            "managed tenant is not admitted"
        );
        Ok(membership(managed.instance.identity.membership))
    }

    pub fn token_binding(&self) -> Result<Option<wire::ManagedAgentToken>> {
        self.managed
            .as_ref()
            .map(|m| {
                Ok(wire::ManagedAgentToken {
                    instance: wire::AgentManagedInstanceId::new(m.instance.key.clone())?,
                    generation: m.instance.active_generation,
                    epoch: m.instance.dispatch_epoch,
                })
            })
            .transpose()
    }

    pub fn apply_service_roles(&self, principal: &mut Principal) -> Result<()> {
        if let Some(managed) = &self.managed {
            principal.roles = managed
                .instance
                .identity
                .roles
                .iter()
                .cloned()
                .map(RoleId::new)
                .collect::<std::result::Result<_, _>>()?;
        }
        Ok(())
    }
}

impl GatewayState {
    pub async fn managed_action_admitted(
        &self,
        catalog: &GatewayCatalog,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> Result<bool> {
        let Some(binding) = &subject.access_token.managed_agent else {
            return Ok(true);
        };
        let Some(client) = self
            .effective_oauth_client(catalog, &subject.access_token.oauth_client_id)
            .await?
        else {
            return Ok(false);
        };
        let Some(managed) = client.managed else {
            return Ok(false);
        };
        if binding.instance.as_str() != managed.instance.key
            || binding.generation != managed.instance.active_generation
        {
            return Ok(false);
        }
        if action == GatewayAction::ToolsCall && binding.epoch != managed.instance.dispatch_epoch {
            return Ok(false);
        }
        if matches!(action, GatewayAction::ToolsCall | GatewayAction::ToolsList)
            && let PolicyTarget::Tool { server, tool } = target
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

    pub async fn effective_oauth_client(
        &self,
        catalog: &GatewayCatalog,
        id: &OAuthClientId,
    ) -> Result<Option<EffectiveOAuthClient>> {
        let managed = self
            .platform
            .managed_agent_registration(id.as_str())
            .await?;
        let installed = catalog.oauth_client(id);
        ensure!(
            managed.is_none() || installed.is_none(),
            "OAuth registration source collision"
        );
        match (installed, managed) {
            (Some(client), None) => Ok(Some(EffectiveOAuthClient {
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
                    .managed_templates
                    .get(&wire::AgentTemplateId::new(template.clone())?)
                    .context("managed template is unavailable")?;
                ensure!(
                    runtime_template_revision(template).as_str()
                        == format!("sha256:{template_revision}"),
                    "managed template changed"
                );
                let instance = &managed.instance;
                let identity = &instance.identity;
                let tenant = TenantId::new(managed.tenant_key.clone())?;
                let context = WorkContextId::new(managed.context_key.clone())?;
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
                    .map(ScopeName::new)
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
                            .map(RoleId::new)
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
                Ok(Some(EffectiveOAuthClient {
                    registration,
                    public_keys: Some(public_keys),
                    managed: Some(managed),
                }))
            }
            _ => Ok(None),
        }
    }

    /// Re-resolve current authority for each authenticated HTTP request, including
    /// tokens minted before definition disable or registration revocation.
    pub async fn resolve_authenticated_subject(
        &self,
        catalog: &GatewayCatalog,
        mut verified: VerifiedAccessToken,
    ) -> Result<AuthenticatedSubject> {
        let client = self
            .effective_oauth_client(catalog, &verified.access_token.oauth_client_id)
            .await?
            .context("OAuth client is unavailable")?;
        ensure!(
            client
                .registration
                .allowed_resources
                .contains(&verified.access_token.audience)
                && verified
                    .access_token
                    .scopes
                    .is_subset(&client.registration.allowed_scopes),
            "current OAuth registration denies token authority"
        );
        match (&client.managed, &verified.access_token.managed_agent) {
            (None, None) => {}
            (Some(managed), Some(binding)) => {
                ensure!(
                    binding.instance.as_str() == managed.instance.key
                        && binding.generation == managed.instance.active_generation
                        && verified.access_token.issuer.as_str()
                            == managed.instance.identity.issuer
                        && verified.access_token.session_family.is_none(),
                    "managed token binding is no longer current"
                );
                client.apply_service_roles(&mut verified.principal)?;
            }
            _ => anyhow::bail!("OAuth token registration source mismatch"),
        }
        let membership = client.membership(
            catalog,
            &verified.access_token.work_context,
            &verified.principal,
        )?;
        Ok(catalog.resolve_subject_with_client(verified, &client.registration, membership)?)
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
