//! Resolve installed-smoke identity from the installation's own control plane.
use veoveo_gateway_contract::ProtectedResourceId;

use std::{fs, path::Path};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::{InstallationClient, InstallationTarget};
use veoveo_mcp_contract::{
    GatewayControlPlane, GatewayProfileId, JwtId, OAuthClientAuthMethod, OAuthClientId,
    OAuthEndpointUrl, OAuthGrantType, WorkContextDefinition,
};
use veoveo_types::{InvocationMode, PrincipalId, ScopeName, TenantId, WorkContextId};

use super::auth::{ClientCredentials, TokenRequest, exchange_token};

pub struct InstalledTarget {
    pub target: InstallationTarget,
    pub operator: InstalledIdentity,
    administrator: Option<InstalledIdentity>,
}

pub struct InstalledIdentity {
    pub client_id: OAuthClientId,
    pub profile: GatewayProfileId,
    pub scopes: Vec<ScopeName>,
    pub resource: ProtectedResourceId,
    pub token_endpoint: OAuthEndpointUrl,
    pub principal: PrincipalId,
    pub invocation_mode: InvocationMode,
    pub comparison_context: Option<WorkContextId>,
    credentials: ClientCredentials,
    pub access_token_key_id: JwtId,
    pub identity_authorization_endpoint: url::Url,
    pub tenant: TenantId,
    pub work_context: WorkContextDefinition,
}

impl InstalledTarget {
    pub fn load(path: &Path) -> Result<Self> {
        let target = InstallationTarget::load(path)?;
        let control_path = target.control_plane_path(path);
        let control: GatewayControlPlane = serde_json::from_slice(
            &fs::read(&control_path)
                .with_context(|| format!("reading control plane {}", control_path.display()))?,
        )
        .context("decoding installation control plane")?;
        Self::from_control_plane(target, control)
    }

    fn from_control_plane(
        target: InstallationTarget,
        control: GatewayControlPlane,
    ) -> Result<Self> {
        target.validate()?;
        control
            .validate(&veoveo_gateway_catalog::registry()?)
            .context("validating installation control plane")?;
        let operator = InstalledIdentity::resolve(
            &target,
            &control,
            &target.operator,
            ClientCredentials::Operator,
        )
        .context("validating installation operator")?;
        let administrator = target
            .administrator
            .as_ref()
            .map(|selection| {
                InstalledIdentity::resolve(
                    &target,
                    &control,
                    selection,
                    ClientCredentials::Administrator,
                )
                .context("validating installation administrator")
            })
            .transpose()?;
        Ok(Self {
            target,
            operator,
            administrator,
        })
    }

    pub fn public_base(&self) -> &str {
        self.target.public_base_url.as_str().trim_end_matches('/')
    }

    pub fn profile(&self) -> &str {
        self.operator.profile.as_str()
    }

    pub fn scopes(&self) -> Vec<&str> {
        self.operator.scopes.iter().map(ScopeName::as_str).collect()
    }

    pub fn administrator(&self) -> Result<&InstalledIdentity> {
        self.administrator
            .as_ref()
            .context("this scenario requires administrator in the installation target")
    }

    pub fn public_url(&self, segments: &[&str]) -> Result<url::Url> {
        let mut url = self.target.public_base_url.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid public origin"))?
            .clear()
            .extend(segments.iter().copied());
        Ok(url)
    }

    pub async fn token(&self) -> Result<String> {
        self.operator.token().await
    }

    pub async fn token_for_context(&self, context: &str) -> Result<String> {
        self.operator
            .token_for_context(&WorkContextId::parse(context)?)
            .await
    }
}

impl InstalledIdentity {
    pub fn validate_credentials(&self) -> Result<()> {
        self.credentials.validate()
    }

    fn resolve(
        target: &InstallationTarget,
        control: &GatewayControlPlane,
        selection: &InstallationClient,
        credentials: ClientCredentials,
    ) -> Result<Self> {
        let profile = control
            .profiles
            .iter()
            .find(|profile| profile.id.as_str() == selection.profile)
            .context("installation client.profile is absent from the control plane")?;
        let mut expected_resource = target.public_base_url.clone();
        expected_resource
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid public origin"))?
            .clear()
            .extend(["mcp", selection.profile.as_str()]);
        ensure!(
            profile.protected_resource.as_str() == expected_resource.as_str(),
            "installation publicBaseUrl and client profile do not match the control plane's protected resource"
        );
        let server = control
            .authorization_servers
            .iter()
            .find(|server| server.id == profile.authorization_server)
            .context("selected profile has no authorization server")?;
        let provider = control
            .identity_providers
            .iter()
            .find(|provider| provider.id == profile.identity_provider)
            .context("selected profile has no identity provider")?;
        let identity_authorization_endpoint = provider
            .authorization_endpoint
            .as_ref()
            .context("selected identity provider has no authorization endpoint")?;
        let identity_authorization_endpoint =
            url::Url::parse(identity_authorization_endpoint.as_str())?;
        let client = control
            .oauth_clients
            .iter()
            .find(|client| client.id.as_str() == selection.client_id)
            .context("installation client.clientId is absent from the control plane")?;
        ensure!(
            client.authorization_server == profile.authorization_server
                && client
                    .allowed_resources
                    .contains(&profile.protected_resource)
                && client
                    .grant_types
                    .contains(&OAuthGrantType::ClientCredentials)
                && client
                    .auth_methods
                    .contains(&OAuthClientAuthMethod::PrivateKeyJwt),
            "selected client must admit private_key_jwt client credentials for the selected profile"
        );
        ensure!(
            selection.scopes.iter().all(|scope| client
                .allowed_scopes
                .iter()
                .any(|allowed| allowed.as_str() == scope)),
            "client.scopes requests a scope not admitted by its client registration"
        );
        ensure!(
            profile.required_scopes.iter().all(|required| selection
                .scopes
                .iter()
                .any(|scope| scope == required.as_str())),
            "client.scopes omits a required profile scope"
        );
        let tenant = client
            .tenant
            .clone()
            .context("selected service client has no tenant")?;
        let work_context = control
            .work_contexts
            .iter()
            .find(|context| {
                context.id.as_str() == selection.work_context && context.tenant == tenant
            })
            .context("selected workContext is absent from its tenant")?
            .clone();
        if let Some(comparison) = &selection.comparison_context {
            ensure!(
                control
                    .work_contexts
                    .iter()
                    .any(|context| context.id.as_str() == comparison && context.tenant == tenant),
                "selected comparisonContext is absent from its tenant"
            );
        }
        Ok(Self {
            client_id: client.id.clone(),
            profile: profile.id.clone(),
            scopes: selection
                .scopes
                .iter()
                .map(ScopeName::parse)
                .collect::<Result<_, _>>()?,
            resource: profile.protected_resource.clone(),
            token_endpoint: server.token_endpoint.clone(),
            principal: PrincipalId::parse(format!("{}#{}", server.issuer, client.id))?,
            invocation_mode: client.invocation_mode,
            comparison_context: selection
                .comparison_context
                .as_ref()
                .map(WorkContextId::parse)
                .transpose()?,
            credentials,
            access_token_key_id: server.access_token_key_id.clone(),
            identity_authorization_endpoint,
            tenant,
            work_context,
        })
    }

    pub async fn token(&self) -> Result<String> {
        self.token_for_context(&self.work_context.id).await
    }

    pub async fn token_for_context(&self, context: &WorkContextId) -> Result<String> {
        ensure!(
            context == &self.work_context.id || self.comparison_context.as_ref() == Some(context),
            "requested smoke Work Context is not declared by the installation target"
        );
        let scopes: Vec<_> = self.scopes.iter().map(ScopeName::as_str).collect();
        exchange_token(TokenRequest {
            token_url: self.token_endpoint.as_str(),
            resource: self.resource.as_str(),
            client_id: self.client_id.as_str(),
            scopes: &scopes,
            work_context: context.as_str(),
            credentials: self.credentials,
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (InstallationTarget, GatewayControlPlane) {
        let target: InstallationTarget = InstallationTarget::decode(include_bytes!(
            "../../../../../testing/fixtures/fork-installation/installation-target.json"
        ))
        .unwrap();
        let control: GatewayControlPlane = serde_json::from_slice(include_bytes!(
            "../../../../../testing/fixtures/fork-installation/gateway.json"
        ))
        .unwrap();
        (target, control)
    }

    #[test]
    fn committed_installation_selections_match_their_control_planes() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        for relative in [
            "examples/bioma/installation-target.json",
            "examples/bioma/installation-target-initial.json",
            "testing/fixtures/fork-installation/installation-target.json",
        ] {
            let path = repository.join(relative);
            let loaded = InstalledTarget::load(&path)
                .unwrap_or_else(|error| panic!("{relative}: {error:#}"));
            let admission = veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
                .bind(veoveo_gateway_catalog::registry().unwrap())
                .unwrap();
            let catalog = veoveo_mcp_gateway::GatewayCatalog::load_json(
                loaded.target.control_plane_path(&path),
                admission,
            )
            .unwrap();
            for identity in std::iter::once(&loaded.operator).chain(loaded.administrator.iter()) {
                let profile = catalog.profile(&identity.profile).unwrap();
                let supported = catalog.profile_supported_scopes(profile);
                for scope in &identity.scopes {
                    assert!(
                        supported.contains(scope),
                        "{relative}: profile {} does not support requested scope {scope}",
                        identity.profile,
                    );
                }
            }
        }
    }

    #[test]
    fn installation_identity_comes_from_the_selected_control_plane() {
        let (target, control) = fixture();
        let expected_key = control.authorization_servers[0].access_token_key_id.clone();
        let loaded = InstalledTarget::from_control_plane(target, control).unwrap();
        assert_eq!(loaded.operator.tenant.as_str(), "enterprise");
        assert_eq!(loaded.operator.access_token_key_id, expected_key);
        assert_eq!(
            loaded.operator.identity_authorization_endpoint.host_str(),
            Some("idp.enterprise.example")
        );
    }

    #[test]
    fn mismatched_origin_client_and_scopes_fail_before_network_access() {
        let (mut target, control) = fixture();
        target.public_base_url = url::Url::parse("https://other.example.test").unwrap();
        assert!(InstalledTarget::from_control_plane(target, control).is_err());
        let (mut target, control) = fixture();
        target.operator.client_id = "unregistered-client".into();
        assert!(InstalledTarget::from_control_plane(target, control).is_err());
        let (mut target, control) = fixture();
        target.operator.scopes.push("unregistered:scope".into());
        assert!(InstalledTarget::from_control_plane(target, control).is_err());
    }
    #[test]
    fn arbitrary_client_profile_and_context_names_resolve_without_reference_defaults() {
        let (target, control) = fixture();
        fn renamed<T: serde::Serialize, U: serde::de::DeserializeOwned>(value: T) -> U {
            fn visit(value: &mut serde_json::Value) {
                match value {
                    serde_json::Value::String(text) => {
                        *text = text
                            .replace("operator-service", "inspection-client")
                            .replace("operator", "inspection-profile")
                            .replace("operations", "inspection-context");
                    }
                    serde_json::Value::Array(values) => values.iter_mut().for_each(visit),
                    serde_json::Value::Object(values) => values.values_mut().for_each(visit),
                    _ => {}
                }
            }
            let mut value = serde_json::to_value(value).unwrap();
            visit(&mut value);
            serde_json::from_value(value).unwrap()
        }
        let loaded =
            InstalledTarget::from_control_plane(renamed(target), renamed(control)).unwrap();
        assert_eq!(loaded.operator.client_id.as_str(), "inspection-client");
        assert_eq!(loaded.profile(), "inspection-profile");
        assert_eq!(
            loaded.operator.work_context.id.as_str(),
            "inspection-context"
        );
        assert_eq!(
            loaded.operator.principal.as_str(),
            "https://localhost:8783/oauth#inspection-client"
        );
        assert_eq!(
            loaded.operator.resource.as_str(),
            "https://localhost:8783/mcp/inspection-profile"
        );
        assert_eq!(loaded.scopes(), ["inspection-profile:use"]);
    }

    #[test]
    fn administrator_selection_is_validated_independently_without_operator_fallback() {
        let (mut target, control) = fixture();
        target.administrator = Some(target.operator.clone());
        let loaded = InstalledTarget::from_control_plane(target.clone(), control.clone()).unwrap();
        assert!(matches!(
            loaded.operator.credentials,
            ClientCredentials::Operator
        ));
        assert!(matches!(
            loaded.administrator().unwrap().credentials,
            ClientCredentials::Administrator
        ));
        target.administrator.as_mut().unwrap().client_id = "missing-admin".into();
        assert!(InstalledTarget::from_control_plane(target, control).is_err());
        let (target, control) = fixture();
        assert!(
            InstalledTarget::from_control_plane(target, control)
                .unwrap()
                .administrator()
                .is_err()
        );
    }

    #[test]
    fn selected_token_endpoint_and_public_path_encoding_are_preserved() {
        let (target, mut control) = fixture();
        let profile = control
            .profiles
            .iter()
            .find(|profile| profile.id.as_str() == target.operator.profile)
            .unwrap();
        let server = control
            .authorization_servers
            .iter_mut()
            .find(|server| server.id == profile.authorization_server)
            .unwrap();
        server.token_endpoint =
            OAuthEndpointUrl::new("https://localhost:8783/machine/exchange").unwrap();
        let loaded = InstalledTarget::from_control_plane(target, control).unwrap();
        assert_eq!(
            loaded.operator.token_endpoint.as_str(),
            "https://localhost:8783/machine/exchange"
        );
        assert_eq!(
            loaded
                .public_url(&["artifacts", loaded.profile(), "a/b?#", "download"])
                .unwrap()
                .as_str(),
            "https://localhost:8783/artifacts/operator/a%2Fb%3F%23/download"
        );
    }
}
