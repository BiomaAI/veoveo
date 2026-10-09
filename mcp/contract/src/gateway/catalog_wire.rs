use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlPlaneWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branding: Option<InstallationBranding>,
    pub identity_providers: Vec<IdentityProvider>,
    pub authorization_servers: Vec<ResourceAuthorizationServer>,
    pub servers: Vec<ServerManifest>,
    pub profiles: Vec<GatewayProfile>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
    pub tenants: Vec<TenantDefinition>,
    pub work_contexts: Vec<crate::WorkContextDefinition>,
    pub policies: Vec<PolicySet>,
    pub data_labels: Vec<DataLabelDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oauth_clients: Vec<OAuthClientRegistration>,
    pub oidc_clients: Vec<IdentityProviderOidcClientRegistration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretReference>,
    #[serde(default)]
    pub metadata: Value,
}
impl<'de> Deserialize<'de> for GatewayControlPlane {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = veoveo_types::UniqueJsonValue::deserialize(deserializer)?.0;
        // Flattened owner extensions cannot absorb retired core spellings.
        for obsolete in [
            "identity_providers",
            "authorization_servers",
            "work_contexts",
            "data_labels",
            "oauth_clients",
            "oidc_clients",
        ] {
            if value.get(obsolete).is_some() {
                return Err(serde::de::Error::custom(format!(
                    "obsolete control-plane field `{obsolete}`"
                )));
            }
        }
        let wire: ControlPlaneWire =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            branding: wire.branding,
            identity_providers: wire.identity_providers,
            authorization_servers: wire.authorization_servers,
            servers: wire.servers,
            profiles: wire.profiles,
            extensions: wire.extensions,
            tenants: wire.tenants,
            work_contexts: wire.work_contexts,
            policies: wire.policies,
            data_labels: wire.data_labels,
            oauth_clients: wire.oauth_clients,
            oidc_clients: wire.oidc_clients,
            secrets: wire.secrets,
            metadata: wire.metadata,
        })
    }
}
impl GatewayControlPlane {
    pub(super) fn catalog_facts(&self) -> veoveo_gateway_contract::CatalogFacts {
        veoveo_gateway_contract::CatalogFacts {
            authorization_servers: self
                .authorization_servers
                .iter()
                .map(|x| x.id.clone())
                .collect(),
            policies: self.policies.iter().map(|x| x.version.clone()).collect(),
            tenants: self.tenants.iter().map(|x| x.id.clone()).collect(),
            data_labels: self.data_labels.iter().map(|x| x.id.clone()).collect(),
            work_contexts: self
                .work_contexts
                .iter()
                .map(|context| veoveo_gateway_contract::WorkContextFacts {
                    id: context.id.clone(),
                    tenant: context.tenant.clone(),
                })
                .collect(),
            oauth_clients: self
                .oauth_clients
                .iter()
                .map(|x| veoveo_gateway_contract::OAuthClientFacts {
                    id: x.id.clone(),
                    authorization_server: x.authorization_server.clone(),
                    tenant: x.tenant.clone(),
                    allowed_resources: x.allowed_resources.clone(),
                    allowed_scopes: x.allowed_scopes.clone(),
                    grant_types: x.grant_types.clone(),
                    auth_methods: x.auth_methods.clone(),
                    default_work_context: x.default_work_context.clone(),
                    invocation_mode: x.invocation_mode,
                    has_jwks: x.jwks.is_some(),
                    has_credential_secret: x.credential_secret.is_some(),
                    has_redirect_uris: !x.redirect_uris.is_empty(),
                    has_compatibility_helpers: !x.allowed_compatibility_helpers.is_empty(),
                    direct_task_call_adapter: x.direct_task_call_adapter,
                    has_knowledge_indexing: x.knowledge_indexing.is_some(),
                })
                .collect(),
        }
    }
}
