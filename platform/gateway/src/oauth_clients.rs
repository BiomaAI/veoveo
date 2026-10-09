//! OAuth registration resolution and contributed current-authority policy.
use crate::{AuthenticatedSubject, GatewayCatalog, GatewayState, VerifiedAccessToken};
use anyhow::{Context, Result, ensure};
use futures::future::BoxFuture;
use jsonwebtoken::jwk::JwkSet;
use std::{fmt::Debug, sync::Arc};
use veoveo_gateway_contract::PolicyAction;
use veoveo_mcp_contract::{OAuthClientId, OAuthClientRegistration, PolicyTarget, Principal};
use veoveo_types::{WorkContextId, WorkContextMembershipLevel};

pub trait OAuthClientResolver: Debug + Send + Sync {
    fn resolve<'a>(
        &'a self,
        catalog: &'a GatewayCatalog,
        id: &'a OAuthClientId,
    ) -> BoxFuture<'a, Result<Option<EffectiveOAuthClient>>>;
}

/// Exclusive authority claimed by an owner for an installed registration.
#[derive(Debug)]
pub enum OAuthClientAuthorityClaim {
    Disabled,
    Active(Arc<dyn OAuthClientAuthority>),
}

/// Claims are exclusive and must include disabled registrations. Returning no
/// claim permits the base resolver's authority to remain available.
pub trait OAuthClientOwner: Debug + Send + Sync {
    fn claim<'a>(
        &'a self,
        catalog: &'a GatewayCatalog,
        id: &'a OAuthClientId,
    ) -> BoxFuture<'a, Result<Option<OAuthClientAuthorityClaim>>>;
}

#[derive(Debug)]
pub struct OwnedOAuthClientResolver {
    base: Arc<dyn OAuthClientResolver>,
    owners: Vec<Arc<dyn OAuthClientOwner>>,
}
impl OwnedOAuthClientResolver {
    pub fn new(base: Arc<dyn OAuthClientResolver>, owners: Vec<Arc<dyn OAuthClientOwner>>) -> Self {
        Self { base, owners }
    }
}
impl OAuthClientResolver for OwnedOAuthClientResolver {
    fn resolve<'a>(
        &'a self,
        catalog: &'a GatewayCatalog,
        id: &'a OAuthClientId,
    ) -> BoxFuture<'a, Result<Option<EffectiveOAuthClient>>> {
        Box::pin(async move {
            // Resolve the base even for disabled claims: its collision and live
            // registration checks must precede any owner decoration.
            let base = self.base.resolve(catalog, id).await;
            let mut claims = Vec::new();
            let mut failure = None;
            for owner in &self.owners {
                match owner.claim(catalog, id).await {
                    Ok(Some(claim)) => claims.push(claim),
                    Ok(None) => {}
                    Err(error) => {
                        if failure.is_none() {
                            failure = Some(error);
                        }
                    }
                }
            }
            let base = base?;
            if let Some(error) = failure {
                return Err(error);
            }
            ensure!(claims.len() <= 1, "OAuth client owner collision");
            let Some(claim) = claims.pop() else {
                return Ok(base);
            };
            let client = base.context("owned OAuth client registration is unavailable")?;
            client.ensure_installed(catalog, id)?;
            match claim {
                OAuthClientAuthorityClaim::Disabled => Ok(None),
                OAuthClientAuthorityClaim::Active(authority) => Ok(Some(
                    client.with_installed_authority(catalog, id, authority)?,
                )),
            }
        })
    }
}

/// Policy supplied by the registration's owner, resolved again for every request.
pub trait OAuthClientAuthority: Debug + Send + Sync {
    fn membership(
        &self,
        catalog: &GatewayCatalog,
        context: &WorkContextId,
        principal: &Principal,
    ) -> Result<WorkContextMembershipLevel>;
    fn token_extensions(&self) -> Result<veoveo_types::AdmittedExtensions>;
    fn execution_attribution(
        &self,
        verified: &VerifiedAccessToken,
    ) -> Result<Option<veoveo_audit_contract::AuditManagedExecution>>;
    fn apply_service_roles(&self, principal: &mut Principal) -> Result<()>;
    fn validate_token(&self, verified: &VerifiedAccessToken) -> Result<()>;
    fn action_admitted(
        &self,
        catalog: &GatewayCatalog,
        subject: &AuthenticatedSubject,
        action: PolicyAction,
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
    fn ensure_installed(&self, catalog: &GatewayCatalog, id: &OAuthClientId) -> Result<()> {
        ensure!(
            self.authority.is_none()
                && self.public_keys.is_none()
                && &self.registration.id == id
                && catalog
                    .oauth_client(id)
                    .is_some_and(|registered| registered == &self.registration),
            "OAuth owner requires an installed registration"
        );
        Ok(())
    }
    pub fn with_installed_authority(
        mut self,
        catalog: &GatewayCatalog,
        id: &OAuthClientId,
        authority: Arc<dyn OAuthClientAuthority>,
    ) -> Result<Self> {
        self.ensure_installed(catalog, id)?;
        self.authority = Some(authority);
        Ok(self)
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
    pub fn token_extensions(&self) -> Result<veoveo_types::AdmittedExtensions> {
        self.authority
            .as_ref()
            .map_or(Ok(Default::default()), |authority| {
                authority.token_extensions()
            })
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
    pub fn bind_token_extensions(
        mut self,
        registry: veoveo_types::ExtensionRegistry,
    ) -> Result<Self> {
        crate::auth::validate_access_token_registry(&registry)?;
        ensure!(
            self.token_extensions.is_none(),
            "token extensions already bound"
        );
        self.token_extensions = Some(registry);
        Ok(self)
    }
    pub fn token_extension_registry(&self) -> Result<veoveo_types::ExtensionRegistry> {
        self.token_extensions
            .clone()
            .context("JWT extension profile is unbound")
    }

    pub fn bind_oauth_client_resolver(
        mut self,
        resolver: Arc<dyn OAuthClientResolver>,
    ) -> Result<Self> {
        ensure!(
            self.token_extensions.is_some(),
            "bind JWT extension profile before OAuth client resolver"
        );
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
        action: impl Into<PolicyAction>,
        target: &PolicyTarget,
    ) -> Result<bool> {
        let Some(client) = self
            .effective_oauth_client(catalog, &subject.access_token.oauth_client_id)
            .await?
        else {
            return Ok(false);
        };
        match &client.authority {
            Some(authority) => authority.action_admitted(catalog, subject, action.into(), target),
            None => {
                Ok(subject.extensions.is_empty()
                    && subject.access_token.managed_execution.is_none())
            }
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
                verified.access_token.managed_execution =
                    authority.execution_attribution(&verified)?;
                client.apply_service_roles(&mut verified.principal)?;
            }
            None => ensure!(
                verified.extensions.is_empty() && verified.access_token.managed_execution.is_none(),
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

#[cfg(test)]
mod owner_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct Authority;
    impl OAuthClientAuthority for Authority {
        fn membership(
            &self,
            _: &GatewayCatalog,
            _: &WorkContextId,
            _: &Principal,
        ) -> Result<WorkContextMembershipLevel> {
            Ok(WorkContextMembershipLevel::Contributor)
        }
        fn token_extensions(&self) -> Result<veoveo_types::AdmittedExtensions> {
            Ok(Default::default())
        }
        fn execution_attribution(
            &self,
            _: &VerifiedAccessToken,
        ) -> Result<Option<veoveo_audit_contract::AuditManagedExecution>> {
            Ok(None)
        }
        fn apply_service_roles(&self, _: &mut Principal) -> Result<()> {
            Ok(())
        }
        fn validate_token(&self, _: &VerifiedAccessToken) -> Result<()> {
            Ok(())
        }
        fn action_admitted(
            &self,
            _: &GatewayCatalog,
            _: &AuthenticatedSubject,
            _: PolicyAction,
            _: &PolicyTarget,
        ) -> Result<bool> {
            Ok(true)
        }
    }
    #[derive(Debug, Clone, Copy)]
    enum Claim {
        Unclaimed,
        Disabled,
        Active,
        Failed,
    }
    #[derive(Debug)]
    struct Owner {
        claim: Claim,
        calls: Arc<AtomicUsize>,
    }
    impl OAuthClientOwner for Owner {
        fn claim<'a>(
            &'a self,
            _: &'a GatewayCatalog,
            _: &'a OAuthClientId,
        ) -> BoxFuture<'a, Result<Option<OAuthClientAuthorityClaim>>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(match self.claim {
                    Claim::Unclaimed => None,
                    Claim::Disabled => Some(OAuthClientAuthorityClaim::Disabled),
                    Claim::Active => Some(OAuthClientAuthorityClaim::Active(Arc::new(Authority))),
                    Claim::Failed => anyhow::bail!("owner admission unavailable"),
                })
            })
        }
    }
    #[derive(Debug)]
    struct Base {
        contributed: bool,
        failed: bool,
        calls: Arc<AtomicUsize>,
    }
    impl OAuthClientResolver for Base {
        fn resolve<'a>(
            &'a self,
            catalog: &'a GatewayCatalog,
            id: &'a OAuthClientId,
        ) -> BoxFuture<'a, Result<Option<EffectiveOAuthClient>>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                ensure!(!self.failed, "base source collision");
                Ok(catalog.oauth_client(id).cloned().map(|registration| {
                    if self.contributed {
                        EffectiveOAuthClient::contributed(registration, None, Arc::new(Authority))
                    } else {
                        EffectiveOAuthClient::installed(registration)
                    }
                }))
            })
        }
    }
    fn catalog() -> GatewayCatalog {
        GatewayCatalog::from_control_plane(
            serde_json::from_str(include_str!("../../../configs/gateway.local.json")).unwrap(),
            crate::catalog_fixture::binding(),
        )
        .unwrap()
    }
    async fn resolve(
        claims: &[Claim],
        contributed: bool,
        failed: bool,
    ) -> (Result<Option<EffectiveOAuthClient>>, usize, usize) {
        let catalog = catalog();
        let id = catalog.control_plane().oauth_clients[0].id.clone();
        let base_calls = Arc::new(AtomicUsize::new(0));
        let owner_calls = Arc::new(AtomicUsize::new(0));
        let resolver = OwnedOAuthClientResolver::new(
            Arc::new(Base {
                contributed,
                failed,
                calls: base_calls.clone(),
            }),
            claims
                .iter()
                .map(|claim| {
                    Arc::new(Owner {
                        claim: *claim,
                        calls: owner_calls.clone(),
                    }) as Arc<dyn OAuthClientOwner>
                })
                .collect(),
        );
        (
            resolver.resolve(&catalog, &id).await,
            base_calls.load(Ordering::SeqCst),
            owner_calls.load(Ordering::SeqCst),
        )
    }
    #[tokio::test]
    async fn disabled_owner_prevents_installed_fallback_and_inspects_every_owner() {
        let (result, base, owners) =
            resolve(&[Claim::Disabled, Claim::Unclaimed], false, false).await;
        assert!(result.unwrap().is_none());
        assert_eq!((base, owners), (1, 2));
        let (result, base, owners) = resolve(
            &[Claim::Disabled, Claim::Failed, Claim::Unclaimed],
            false,
            false,
        )
        .await;
        assert!(result.is_err());
        assert_eq!((base, owners), (1, 3));
    }
    #[tokio::test]
    async fn duplicate_claims_and_base_collisions_fail_closed() {
        assert!(
            resolve(&[Claim::Disabled, Claim::Active], false, false)
                .await
                .0
                .is_err()
        );
        let (result, base, owners) = resolve(&[Claim::Disabled], false, true).await;
        assert!(result.is_err());
        assert_eq!((base, owners), (1, 1));
    }
    #[tokio::test]
    async fn owner_only_attaches_to_installed_registration() {
        let client = resolve(&[Claim::Active], false, false)
            .await
            .0
            .unwrap()
            .unwrap();
        assert!(client.authority.is_some());
        assert!(resolve(&[Claim::Active], true, false).await.0.is_err());
        assert!(resolve(&[Claim::Disabled], true, false).await.0.is_err());
        let client = resolve(&[Claim::Unclaimed], true, false)
            .await
            .0
            .unwrap()
            .unwrap();
        assert!(
            client.authority.is_some(),
            "unclaimed managed authority must survive"
        );
        let client = resolve(&[Claim::Unclaimed], false, false)
            .await
            .0
            .unwrap()
            .unwrap();
        assert!(
            client.authority.is_none(),
            "unclaimed installed authority must survive"
        );
    }
}
