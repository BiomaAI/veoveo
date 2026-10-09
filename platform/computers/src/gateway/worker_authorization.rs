//! Current installation authority for the isolated provider-worker OAuth client.
use anyhow::{Context, Result, ensure};
use futures::future::BoxFuture;
use std::sync::Arc;
use veoveo_computers_contract::{
    COMPUTER_WORKER_AUTHORIZATION_SECTION, ComputerWorkerAuthorizationSection,
};
use veoveo_gateway_contract::{OAuthClientAuthMethod, OAuthGrantType, PolicyAction};
use veoveo_mcp_contract::{OAuthClientId, PolicyTarget, Principal, PrincipalKind, TokenIssuer};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, GatewayCatalog, VerifiedAccessToken,
    oauth_clients::{OAuthClientAuthority, OAuthClientAuthorityClaim, OAuthClientOwner},
};
use veoveo_types::{ExtensionName, InvocationMode, WorkContextId, WorkContextMembershipLevel};

#[derive(Debug, Default)]
pub struct ComputerWorkerOAuthOwner;
impl ComputerWorkerOAuthOwner {
    pub fn new() -> Self {
        Self
    }
}

impl OAuthClientOwner for ComputerWorkerOAuthOwner {
    fn claim<'a>(
        &'a self,
        catalog: &'a GatewayCatalog,
        id: &'a OAuthClientId,
    ) -> BoxFuture<'a, Result<Option<OAuthClientAuthorityClaim>>> {
        Box::pin(async move {
            let key = catalog
                .registry()
                .section_key::<ComputerWorkerAuthorizationSection>(&ExtensionName::parse(
                    COMPUTER_WORKER_AUTHORIZATION_SECTION,
                )?)?;
            let Some(section) = catalog.sections().get(&key)? else {
                return Ok(None);
            };
            let f = section.fields();
            if &f.client_id != id {
                return Ok(None);
            }
            // An explicitly disabled registration remains claimed; no installed fallback.
            if !f.enabled {
                return Ok(Some(OAuthClientAuthorityClaim::Disabled));
            }
            let client = catalog
                .oauth_client(id)
                .context("Computer worker registration is absent")?;
            ensure!(
                client.authorization_server == f.authorization_server
                    && client.tenant.as_ref() == Some(&f.tenant)
                    && client.default_work_context == f.work_context
                    && client.invocation_mode == InvocationMode::Automated
                    && client.allowed_resources == [f.resource.clone()].into()
                    && client.allowed_scopes == section.scopes()
                    && client.grant_types == [OAuthGrantType::ClientCredentials].into()
                    && client.auth_methods == [OAuthClientAuthMethod::PrivateKeyJwt].into()
                    && client.jwks.is_some()
                    && client.credential_secret.is_none()
                    && client.redirect_uris.is_empty()
                    && client.allowed_compatibility_helpers.is_empty()
                    && !client.direct_task_call_adapter
                    && client.knowledge_indexing.is_none(),
                "Computer worker registration does not match its current isolated authority"
            );
            let context = catalog
                .work_context(&f.work_context)
                .context("Computer worker context is absent")?;
            ensure!(
                context.tenant == f.tenant,
                "Computer worker context belongs to another tenant"
            );
            let issuer = catalog
                .authorization_server(&f.authorization_server)
                .context("Computer worker authorization server is absent")?
                .issuer
                .clone();
            Ok(Some(OAuthClientAuthorityClaim::Active(Arc::new(
                ComputerWorkerAuthority { section, issuer },
            ))))
        })
    }
}

#[derive(Debug)]
struct ComputerWorkerAuthority {
    section: ComputerWorkerAuthorizationSection,
    issuer: TokenIssuer,
}
impl ComputerWorkerAuthority {
    fn service(&self, principal: &Principal) -> Result<()> {
        let f = self.section.fields();
        ensure!(
            principal.kind == PrincipalKind::Service
                && principal.issuer == self.issuer
                && principal.subject.as_str() == f.client_id.as_str()
                && principal.tenant.as_ref() == Some(&f.tenant)
                && principal.scopes == self.section.scopes(),
            "Computer worker service authority differs"
        );
        Ok(())
    }
}
impl OAuthClientAuthority for ComputerWorkerAuthority {
    fn membership(
        &self,
        catalog: &GatewayCatalog,
        context: &WorkContextId,
        principal: &Principal,
    ) -> Result<WorkContextMembershipLevel> {
        self.service(principal)?;
        let f = self.section.fields();
        ensure!(
            context == &f.work_context,
            "Computer worker cannot select another Work Context"
        );
        Ok(catalog.work_context_membership(&f.client_id, context, principal)?)
    }
    fn token_extensions(&self) -> Result<veoveo_types::AdmittedExtensions> {
        Ok(Default::default())
    }
    fn execution_attribution(
        &self,
        verified: &VerifiedAccessToken,
    ) -> Result<Option<veoveo_mcp_contract::audit::AuditManagedExecution>> {
        self.validate_token(verified)?;
        Ok(None)
    }
    fn apply_service_roles(&self, principal: &mut Principal) -> Result<()> {
        self.service(principal)?;
        principal.roles = self.section.roles();
        Ok(())
    }
    fn validate_token(&self, verified: &VerifiedAccessToken) -> Result<()> {
        self.service(&verified.principal)?;
        let f = self.section.fields();
        ensure!(
            verified.access_token.oauth_client_id == f.client_id
                && verified.access_token.audience == f.resource
                && verified.access_token.work_context == f.work_context
                && verified.access_token.scopes == self.section.scopes()
                && verified.access_token.invocation_mode == InvocationMode::Automated
                && verified.access_token.session_family.is_none()
                && verified.access_token.initiator.is_none()
                && verified.access_token.delegation_id.is_none()
                && verified.principal.roles == self.section.roles()
                && verified.extensions.is_empty()
                && verified.access_token.managed_execution.is_none(),
            "Computer worker token differs from current registration"
        );
        Ok(())
    }
    fn action_admitted(
        &self,
        _catalog: &GatewayCatalog,
        _subject: &AuthenticatedSubject,
        _action: PolicyAction,
        _target: &PolicyTarget,
    ) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;
    use veoveo_mcp_contract::{GatewayControlPlane, TokenSubject};
    use veoveo_types::PrincipalId;
    fn catalog(enabled: bool, member: bool) -> GatewayCatalog {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../../../../configs/gateway.smoke.json")).unwrap();
        value["oauthClients"].as_array_mut().unwrap().push(json!({"id":"worker","authorizationServer":"veoveo","defaultWorkContext":"operations","invocationMode":"automated",
            "allowedResources":["https://veoveo.example/computers/provider"],"grantTypes":["client_credentials"],"authMethods":["private_key_jwt"],
            "allowedScopes":["computers:provider:authenticate"],"jwks":{"source":"remote","jwksUri":"https://veoveo.example/worker-jwks"},"tenant":"tenant-a"}));
        value[COMPUTER_WORKER_AUTHORIZATION_SECTION] = json!({"resourceName":"computer-provider","resource":"https://veoveo.example/computers/provider",
            "authorizationServer":"veoveo","policyVersion":"2026-07-02","clientId":"worker","tenant":"tenant-a","workContext":"operations","enabled":enabled,
            "adminRole":"worker-admin","userRole":"worker-user","grantedRoles":["user","admin"]});
        if member {
            value["workContexts"][0]["memberships"]
                .as_array_mut()
                .unwrap()
                .push(json!({"level":"contributor","oauthClients":["worker"]}));
        }
        let control: GatewayControlPlane = serde_json::from_value(value).unwrap();
        GatewayCatalog::from_control_plane(control, crate::test_catalog_admission::binding())
            .unwrap()
    }
    fn principal(issuer: TokenIssuer, scopes: BTreeSet<veoveo_types::ScopeName>) -> Principal {
        Principal {
            id: PrincipalId::parse("https://veoveo.example/oauth#worker").unwrap(),
            kind: PrincipalKind::Service,
            issuer,
            subject: TokenSubject::parse("worker").unwrap(),
            tenant: Some("tenant-a".parse().unwrap()),
            groups: Default::default(),
            group_roles: Default::default(),
            roles: Default::default(),
            scopes,
            data_labels: Default::default(),
            assurances: Default::default(),
            authenticated_at: None,
        }
    }
    #[tokio::test]
    async fn owner_claims_disabled_clients_and_scopes_active_service_roles() {
        let owner = ComputerWorkerOAuthOwner::new();
        let id = "worker".parse().unwrap();
        assert!(matches!(
            owner.claim(&catalog(false, true), &id).await.unwrap(),
            Some(OAuthClientAuthorityClaim::Disabled)
        ));
        assert!(
            owner
                .claim(
                    &catalog(true, true),
                    &"operator-local-public".parse().unwrap()
                )
                .await
                .unwrap()
                .is_none()
        );
        let c = catalog(true, true);
        let Some(OAuthClientAuthorityClaim::Active(authority)) =
            owner.claim(&c, &id).await.unwrap()
        else {
            panic!("active owner")
        };
        let mut p = principal(
            "https://veoveo.example/oauth".parse().unwrap(),
            ["computers:provider:authenticate".parse().unwrap()].into(),
        );
        authority.apply_service_roles(&mut p).unwrap();
        assert_eq!(
            p.roles,
            [
                "worker-user".parse().unwrap(),
                "worker-admin".parse().unwrap()
            ]
            .into()
        );
        assert!(
            authority
                .membership(&c, &"operations".parse().unwrap(), &p)
                .is_ok()
        );
        assert!(
            authority
                .membership(&catalog(true, false), &"operations".parse().unwrap(), &p)
                .is_err()
        );
        assert!(
            authority
                .membership(&c, &"other".parse().unwrap(), &p)
                .is_err()
        );
        p.kind = PrincipalKind::User;
        assert!(authority.apply_service_roles(&mut p).is_err());
        assert!(authority.token_extensions().unwrap().is_empty());
    }
    #[tokio::test]
    async fn worker_token_rejects_other_audiences_roles_and_gateway_actions() {
        use chrono::{TimeDelta, Utc};
        use veoveo_gateway_contract::GatewayAction;
        use veoveo_mcp_contract::AccessTokenSubject;
        let c = catalog(true, true);
        let Some(OAuthClientAuthorityClaim::Active(authority)) = ComputerWorkerOAuthOwner::new()
            .claim(&c, &"worker".parse().unwrap())
            .await
            .unwrap()
        else {
            panic!("active owner")
        };
        let mut p = principal(
            "https://veoveo.example/oauth".parse().unwrap(),
            ["computers:provider:authenticate".parse().unwrap()].into(),
        );
        authority.apply_service_roles(&mut p).unwrap();
        let now = Utc::now();
        let token = AccessTokenSubject {
            issuer: p.issuer.clone(),
            subject: p.subject.clone(),
            oauth_client_id: "worker".parse().unwrap(),
            session_family: None,
            audience: "https://veoveo.example/computers/provider".parse().unwrap(),
            work_context: "operations".parse().unwrap(),
            invocation_mode: InvocationMode::Automated,
            initiator: None,
            delegation_id: None,
            scopes: p.scopes.clone(),
            jwt_id: None,
            issued_at: now,
            not_before: None,
            expires_at: now + TimeDelta::seconds(900),
            managed_execution: None,
        };
        let verified = VerifiedAccessToken {
            extensions: Default::default(),
            access_token: token,
            principal: p.clone(),
            principal_display_name: None,
        };
        authority.validate_token(&verified).unwrap();
        let mut other = verified.clone();
        other.access_token.audience = "https://veoveo.example/mcp/operator".parse().unwrap();
        assert!(authority.validate_token(&other).is_err());
        let mut other = verified.clone();
        other.principal.roles.insert("operator".parse().unwrap());
        assert!(authority.validate_token(&other).is_err());
        let mut other = verified.clone();
        other.access_token.oauth_client_id = "operator-local-public".parse().unwrap();
        assert!(authority.validate_token(&other).is_err());
        let mut registry =
            veoveo_types::ExtensionRegistryBuilder::new(std::iter::empty::<String>());
        let extension = ExtensionName::parse("ai.veoveo/test-worker-token").unwrap();
        registry.reserve(extension.clone()).unwrap();
        registry.bind_serde::<bool>(&extension).unwrap();
        let mut other = verified.clone();
        other.extensions = registry
            .build()
            .admit([(extension.as_str().to_owned(), json!(true))].into())
            .unwrap();
        assert!(authority.validate_token(&other).is_err());
        let invocation =
            serde_json::from_value(json!({"work_context":"operations","tenant":"tenant-a",
            "membership":"contributor","policy_revision":"2026-07-02",
            "output_policy":{"owner":{"kind":"principal","id":p.id}},
            "provenance":{"mode":"automated"}}))
            .unwrap();
        let subject = AuthenticatedSubject {
            extensions: Default::default(),
            audit: veoveo_mcp_contract::audit::AuditRequest::background(),
            access_token: verified.access_token,
            principal: p.clone(),
            actor: p,
            principal_display_name: None,
            authority: invocation,
        };
        assert!(
            !authority
                .action_admitted(
                    &c,
                    &subject,
                    PolicyAction::Kernel(GatewayAction::ResourcesRead),
                    &PolicyTarget::Gateway
                )
                .unwrap()
        );
    }
}
