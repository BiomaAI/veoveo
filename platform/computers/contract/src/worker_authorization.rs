//! Installation-owned provider-worker authority, separate from public Computer scopes.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_gateway_contract::{
    AuthorizationServerId, CatalogFacts, CatalogSection, OAuthClientAuthMethod, OAuthGrantType,
    ProtectedResourceDescriptor, ProtectedResourceId, ProtectedResourceName,
};
use veoveo_types::{
    Check, Checked, ExtensionError, HttpsUrl, InvocationMode, OAuthClientId, PolicyVersion, RoleId,
    ScopeDefinition, ScopeName, TenantId, WorkContextId,
};

pub const COMPUTER_WORKER_AUTHORIZATION_SECTION: &str = "ai.veoveo/computer-worker-authorization";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum ComputerWorkerScope {
    #[vocabulary(rename = "computers:provider:authenticate")]
    Authenticate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, veoveo_types::Vocabulary)]
pub enum ComputerWorkerRole {
    User,
    Admin,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComputerWorkerAuthorizationFields {
    pub resource_name: ProtectedResourceName,
    pub resource: ProtectedResourceId,
    pub authorization_server: AuthorizationServerId,
    pub policy_version: PolicyVersion,
    pub client_id: OAuthClientId,
    pub tenant: TenantId,
    pub work_context: WorkContextId,
    pub enabled: bool,
    pub admin_role: RoleId,
    pub user_role: RoleId,
    pub granted_roles: BTreeSet<ComputerWorkerRole>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid Computer worker authorization profile")]
pub struct ComputerWorkerAuthorizationError;

impl Check for ComputerWorkerAuthorizationFields {
    type Error = ComputerWorkerAuthorizationError;
    fn check(&self) -> Result<(), Self::Error> {
        let url = self
            .resource
            .as_str()
            .parse::<HttpsUrl>()
            .map_err(|_| ComputerWorkerAuthorizationError)?;
        if url.as_str() != self.resource.as_str()
            || url.as_url().query().is_some()
            || url.as_url().fragment().is_some()
            || self.granted_roles.is_empty()
            || self.admin_role == self.user_role
            || self.client_id.as_str().len() > 256
        {
            return Err(ComputerWorkerAuthorizationError);
        }
        for role in [&self.admin_role, &self.user_role] {
            if role.as_str().len() > 256
                || role
                    .as_str()
                    .bytes()
                    .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
            {
                return Err(ComputerWorkerAuthorizationError);
            }
        }
        Ok(())
    }
}
impl ComputerWorkerAuthorizationFields {
    pub fn build(
        self,
    ) -> Result<ComputerWorkerAuthorizationSection, ComputerWorkerAuthorizationError> {
        Checked::new(self).map(ComputerWorkerAuthorizationSection)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ComputerWorkerAuthorizationSection(Checked<ComputerWorkerAuthorizationFields>);
impl ComputerWorkerAuthorizationSection {
    pub fn fields(&self) -> &ComputerWorkerAuthorizationFields {
        self.0.get()
    }
    pub fn scopes(&self) -> BTreeSet<ScopeName> {
        [ComputerWorkerScope::Authenticate.name().clone()].into()
    }
    pub fn roles(&self) -> BTreeSet<RoleId> {
        self.0
            .granted_roles
            .iter()
            .map(|role| match role {
                ComputerWorkerRole::User => self.0.user_role.clone(),
                ComputerWorkerRole::Admin => self.0.admin_role.clone(),
            })
            .collect()
    }
}
impl CatalogSection for ComputerWorkerAuthorizationSection {
    fn validate(&self, facts: &CatalogFacts) -> Result<(), ExtensionError> {
        let f = self.fields();
        let fail = || {
            ExtensionError::new(
                "Computer worker client must be an isolated automated private_key_jwt registration with current tenant/context",
            )
        };
        let client = facts
            .oauth_clients
            .iter()
            .find(|client| client.id == f.client_id)
            .ok_or_else(fail)?;
        if !facts
            .authorization_servers
            .contains(&f.authorization_server)
            || !facts.policies.contains(&f.policy_version)
            || !facts.tenants.contains(&f.tenant)
            || !facts
                .work_contexts
                .iter()
                .any(|context| context.id == f.work_context && context.tenant == f.tenant)
            || client.authorization_server != f.authorization_server
            || client.tenant.as_ref() != Some(&f.tenant)
            || client.default_work_context != f.work_context
            || client.invocation_mode != InvocationMode::Automated
            || client.allowed_resources != [f.resource.clone()].into()
            || client.allowed_scopes != self.scopes()
            || client.grant_types != [OAuthGrantType::ClientCredentials].into()
            || client.auth_methods != [OAuthClientAuthMethod::PrivateKeyJwt].into()
            || !client.has_jwks
            || client.has_credential_secret
            || client.has_redirect_uris
            || client.has_compatibility_helpers
            || client.direct_task_call_adapter
            || client.has_knowledge_indexing
        {
            return Err(fail());
        }
        Ok(())
    }
    fn protected_resources(&self) -> Vec<ProtectedResourceDescriptor> {
        let f = self.fields();
        vec![ProtectedResourceDescriptor {
            name: f.resource_name.clone(),
            resource: f.resource.clone(),
            authorization_server: f.authorization_server.clone(),
            policy_version: f.policy_version.clone(),
            required_scopes: self.scopes(),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_gateway_contract::{OAuthClientFacts, WorkContextFacts};
    fn fields() -> ComputerWorkerAuthorizationFields {
        serde_json::from_value(serde_json::json!({"resourceName":"computer-provider", "resource":"https://worker.example/computers/provider",
            "authorizationServer":"veoveo", "policyVersion":"worker-policy", "clientId":"worker", "tenant":"tenant", "workContext":"operations",
            "enabled":true,"adminRole":"worker-admin","userRole":"worker-user","grantedRoles":["user","admin"]})).unwrap()
    }
    fn facts(section: &ComputerWorkerAuthorizationSection) -> CatalogFacts {
        let f = section.fields();
        CatalogFacts {
            authorization_servers: [f.authorization_server.clone()].into(),
            policies: [f.policy_version.clone()].into(),
            tenants: [f.tenant.clone()].into(),
            work_contexts: vec![WorkContextFacts {
                id: f.work_context.clone(),
                tenant: f.tenant.clone(),
            }],
            oauth_clients: vec![OAuthClientFacts {
                id: f.client_id.clone(),
                authorization_server: f.authorization_server.clone(),
                tenant: Some(f.tenant.clone()),
                allowed_resources: [f.resource.clone()].into(),
                allowed_scopes: section.scopes(),
                grant_types: [OAuthGrantType::ClientCredentials].into(),
                auth_methods: [OAuthClientAuthMethod::PrivateKeyJwt].into(),
                default_work_context: f.work_context.clone(),
                invocation_mode: InvocationMode::Automated,
                has_jwks: true,
                has_credential_secret: false,
                has_redirect_uris: false,
                has_compatibility_helpers: false,
                direct_task_call_adapter: false,
                has_knowledge_indexing: false,
            }],
            ..CatalogFacts::default()
        }
    }
    #[test]
    fn builder_and_decoder_check_https_role_relationships() {
        let section = fields().build().unwrap();
        assert_eq!(
            serde_json::from_value::<ComputerWorkerAuthorizationSection>(
                serde_json::to_value(&section).unwrap()
            )
            .unwrap(),
            section
        );
        for case in 0..4 {
            let mut f = fields();
            match case {
                0 => f.resource = "http://worker.example/provider".parse().unwrap(),
                1 => f.admin_role = f.user_role.clone(),
                2 => f.granted_roles.clear(),
                _ => f.resource = "https://worker.example/provider?token=x".parse().unwrap(),
            }
            assert!(f.clone().build().is_err());
            assert!(
                serde_json::from_value::<ComputerWorkerAuthorizationSection>(
                    serde_json::to_value(&f).unwrap()
                )
                .is_err()
            );
        }
    }
    #[test]
    fn catalog_requires_exact_private_worker_authority() {
        let section = fields().build().unwrap();
        section.validate(&facts(&section)).unwrap();
        for case in 0..15 {
            let mut facts = facts(&section);
            let client = &mut facts.oauth_clients[0];
            match case {
                0 => {
                    client
                        .grant_types
                        .insert(OAuthGrantType::AuthorizationCodePkce);
                }
                1 => {
                    client.auth_methods.insert(OAuthClientAuthMethod::None);
                }
                2 => {
                    client
                        .allowed_resources
                        .insert("https://worker.example/mcp/operator".parse().unwrap());
                }
                3 => client.allowed_scopes.clear(),
                4 => client.tenant = None,
                5 => client.default_work_context = "another-context".parse().unwrap(),
                6 => client.invocation_mode = InvocationMode::Direct,
                7 => client.has_jwks = false,
                8 => client.has_credential_secret = true,
                9 => client.has_redirect_uris = true,
                10 => client.has_compatibility_helpers = true,
                11 => client.direct_task_call_adapter = true,
                12 => client.has_knowledge_indexing = true,
                13 => facts.work_contexts[0].tenant = "another-tenant".parse().unwrap(),
                _ => facts.policies.clear(),
            }
            assert!(section.validate(&facts).is_err(), "case {case}");
        }
        let mut f = fields();
        f.enabled = false;
        f.build().unwrap().validate(&facts(&section)).unwrap();
    }
}
