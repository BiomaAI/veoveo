//! Resolve installed-smoke identity from the installation's own control plane.

use std::{fs, path::Path};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::InstallationTarget;
use veoveo_mcp_contract::{
    GatewayControlPlane, JwtId, OAuthClientAuthMethod, OAuthGrantType, WorkContextDefinition,
};
use veoveo_types::TenantId;

use super::gateway_token_for_context;

pub(crate) struct InstalledTarget {
    pub target: InstallationTarget,
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
            .validate()
            .context("validating installation control plane")?;
        let operator = &target.operator;
        let profile = control
            .profiles
            .iter()
            .find(|profile| profile.id.as_str() == operator.profile)
            .context("installation operator.profile is absent from the control plane")?;
        let expected_resource = format!(
            "{}/mcp/{}",
            target.public_base_url.as_str().trim_end_matches('/'),
            operator.profile
        );
        ensure!(
            profile.protected_resource.as_str() == expected_resource,
            "installation publicBaseUrl and operator.profile do not match the control plane's protected resource"
        );
        let server = control
            .authorization_servers
            .iter()
            .find(|server| server.id == profile.authorization_server)
            .context("operator profile has no authorization server")?;
        let provider = control
            .identity_providers
            .iter()
            .find(|provider| provider.id == profile.identity_provider)
            .context("operator profile has no identity provider")?;
        let identity_authorization_endpoint = provider
            .authorization_endpoint
            .as_ref()
            .context("operator identity provider has no authorization endpoint")?;
        let identity_authorization_endpoint =
            url::Url::parse(identity_authorization_endpoint.as_str())?;
        let client = control
            .oauth_clients
            .iter()
            .find(|client| client.id.as_str() == operator.client_id)
            .context("installation operator.clientId is absent from the control plane")?;
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
            "operator client must admit private_key_jwt client credentials for the selected profile"
        );
        ensure!(
            operator.scopes.iter().all(|scope| client
                .allowed_scopes
                .iter()
                .any(|allowed| allowed.as_str() == scope)),
            "operator.scopes requests a scope not admitted by its client registration"
        );
        ensure!(
            profile.required_scopes.iter().all(|required| operator
                .scopes
                .iter()
                .any(|scope| scope == required.as_str())),
            "operator.scopes omits a required profile scope"
        );
        let tenant = client
            .tenant
            .clone()
            .context("operator service client has no tenant")?;
        let work_context = control
            .work_contexts
            .iter()
            .find(|context| {
                context.id.as_str() == operator.work_context && context.tenant == tenant
            })
            .context("operator workContext is absent from its tenant")?
            .clone();
        if let Some(comparison) = &operator.comparison_context {
            ensure!(
                control
                    .work_contexts
                    .iter()
                    .any(|context| context.id.as_str() == comparison && context.tenant == tenant),
                "operator comparisonContext is absent from its tenant"
            );
        }
        let access_token_key_id = server.access_token_key_id.clone();
        Ok(Self {
            target,
            access_token_key_id,
            identity_authorization_endpoint,
            tenant,
            work_context,
        })
    }

    pub fn public_base(&self) -> &str {
        self.target.public_base_url.as_str().trim_end_matches('/')
    }

    pub fn profile(&self) -> &str {
        &self.target.operator.profile
    }

    pub fn scopes(&self) -> Vec<&str> {
        self.target
            .operator
            .scopes
            .iter()
            .map(String::as_str)
            .collect()
    }

    pub async fn token(&self, conformance: &Path) -> Result<String> {
        self.token_for_context(conformance, &self.target.operator.work_context)
            .await
    }

    pub async fn token_for_context(&self, conformance: &Path, context: &str) -> Result<String> {
        ensure!(
            context == self.target.operator.work_context
                || self.target.operator.comparison_context.as_deref() == Some(context),
            "requested smoke Work Context is not declared by the installation target"
        );
        gateway_token_for_context(
            conformance,
            self.public_base(),
            &self.target.operator.client_id,
            self.profile(),
            &self.scopes(),
            context,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (InstallationTarget, GatewayControlPlane) {
        let target: InstallationTarget = InstallationTarget::decode(include_bytes!(
            "../../../../../fixtures/fork-installation/installation-target.json"
        ))
        .unwrap();
        let control: GatewayControlPlane = serde_json::from_slice(include_bytes!(
            "../../../../../fixtures/fork-installation/gateway.json"
        ))
        .unwrap();
        (target, control)
    }

    #[test]
    fn installation_identity_comes_from_the_selected_control_plane() {
        let (target, control) = fixture();
        let expected_key = control.authorization_servers[0].access_token_key_id.clone();
        let loaded = InstalledTarget::from_control_plane(target, control).unwrap();
        assert_eq!(loaded.tenant.as_str(), "enterprise");
        assert_eq!(loaded.access_token_key_id, expected_key);
        assert_eq!(
            loaded.identity_authorization_endpoint.host_str(),
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
}
