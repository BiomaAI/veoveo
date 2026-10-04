//! OAuth registration resolution and contributed current-authority policy.
use crate::{AuthenticatedSubject, GatewayCatalog, GatewayState, VerifiedAccessToken};
use anyhow::{Context, Result, ensure};
use futures::future::BoxFuture;
use jsonwebtoken::jwk::JwkSet;
use std::{fmt::Debug, sync::Arc};
use veoveo_mcp_contract::{
    GatewayAction, OAuthClientId, OAuthClientRegistration, PolicyTarget, Principal,
    agent_management::ManagedAgentToken,
};
use veoveo_types::{WorkContextId, WorkContextMembershipLevel};

pub trait OAuthClientResolver: Debug + Send + Sync {
    fn resolve<'a>(
        &'a self,
        catalog: &'a GatewayCatalog,
        id: &'a OAuthClientId,
    ) -> BoxFuture<'a, Result<Option<EffectiveOAuthClient>>>;
}

/// Policy supplied by the registration's owner, resolved again for every request.
pub trait OAuthClientAuthority: Debug + Send + Sync {
    fn membership(
        &self,
        catalog: &GatewayCatalog,
        context: &WorkContextId,
        principal: &Principal,
    ) -> Result<WorkContextMembershipLevel>;
    fn token_binding(&self) -> Result<Option<ManagedAgentToken>>;
    fn apply_service_roles(&self, principal: &mut Principal) -> Result<()>;
    fn validate_token(&self, verified: &VerifiedAccessToken) -> Result<()>;
    fn action_admitted(
        &self,
        catalog: &GatewayCatalog,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> Result<bool>;
}

#[derive(Debug, Clone)]
pub struct EffectiveOAuthClient {
    pub registration: OAuthClientRegistration,
    pub public_keys: Option<JwkSet>,
    authority: Option<Arc<dyn OAuthClientAuthority>>,
}
impl EffectiveOAuthClient {
    pub fn installed(registration: OAuthClientRegistration) -> Self {
        Self {
            registration,
            public_keys: None,
            authority: None,
        }
    }
    pub fn contributed(
        registration: OAuthClientRegistration,
        public_keys: Option<JwkSet>,
        authority: Arc<dyn OAuthClientAuthority>,
    ) -> Self {
        Self {
            registration,
            public_keys,
            authority: Some(authority),
        }
    }
    pub fn membership(
        &self,
        catalog: &GatewayCatalog,
        context: &WorkContextId,
        principal: &Principal,
    ) -> Result<WorkContextMembershipLevel> {
        match &self.authority {
            Some(authority) => authority.membership(catalog, context, principal),
            None => {
                Ok(catalog.work_context_membership(&self.registration.id, context, principal)?)
            }
        }
    }
    pub fn token_binding(&self) -> Result<Option<ManagedAgentToken>> {
        self.authority
            .as_ref()
            .map_or(Ok(None), |authority| authority.token_binding())
    }
    pub fn apply_service_roles(&self, principal: &mut Principal) -> Result<()> {
        self.authority
            .as_ref()
            .map_or(Ok(()), |authority| authority.apply_service_roles(principal))
    }
}

/// Explicitly admits only installed catalog clients. Composition selects this
/// resolver only when durable module registrations are outside its supported profile.
#[derive(Debug)]
pub struct CatalogOAuthClientResolver;
impl OAuthClientResolver for CatalogOAuthClientResolver {
    fn resolve<'a>(
        &'a self,
        catalog: &'a GatewayCatalog,
        id: &'a OAuthClientId,
    ) -> BoxFuture<'a, Result<Option<EffectiveOAuthClient>>> {
        Box::pin(async move {
            Ok(catalog
                .oauth_client(id)
                .cloned()
                .map(EffectiveOAuthClient::installed))
        })
    }
}
impl GatewayState {
    pub fn bind_oauth_client_resolver(
        mut self,
        resolver: Arc<dyn OAuthClientResolver>,
    ) -> Result<Self> {
        ensure!(
            self.oauth_client_resolver.is_none(),
            "OAuth client resolver already bound"
        );
        self.oauth_client_resolver = Some(resolver);
        Ok(self)
    }
    pub async fn effective_oauth_client(
        &self,
        catalog: &GatewayCatalog,
        id: &OAuthClientId,
    ) -> Result<Option<EffectiveOAuthClient>> {
        self.oauth_client_resolver
            .as_ref()
            .context(
                "OAuth client resolver is unbound; bind the installation registration adapter",
            )?
            .resolve(catalog, id)
            .await
    }
    pub async fn oauth_action_admitted(
        &self,
        catalog: &GatewayCatalog,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> Result<bool> {
        let Some(client) = self
            .effective_oauth_client(catalog, &subject.access_token.oauth_client_id)
            .await?
        else {
            return Ok(false);
        };
        match &client.authority {
            Some(authority) => authority.action_admitted(catalog, subject, action, target),
            None => Ok(subject.access_token.managed_agent.is_none()),
        }
    }
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
        match &client.authority {
            Some(authority) => {
                authority.validate_token(&verified)?;
                client.apply_service_roles(&mut verified.principal)?;
            }
            None => ensure!(
                verified.access_token.managed_agent.is_none(),
                "OAuth token registration source mismatch"
            ),
        }
        let membership = client.membership(
            catalog,
            &verified.access_token.work_context,
            &verified.principal,
        )?;
        Ok(catalog.resolve_subject_with_client(verified, &client.registration, membership)?)
    }
}
