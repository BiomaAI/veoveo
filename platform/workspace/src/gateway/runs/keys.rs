use secrecy::SecretString;
use veoveo_mcp_contract::{SecretPurpose, SecretReferenceId};
use veoveo_mcp_gateway::{GatewayCatalog, GatewaySecretResolver};
use veoveo_platform_store::workspace::WorkspaceRunFailure;

#[derive(Clone, Default)]
pub(super) struct ModelKeys {
    #[cfg(test)]
    pub fixture: Option<SecretString>,
}
impl ModelKeys {
    pub async fn resolve(
        &self,
        catalog: &GatewayCatalog,
        id: &SecretReferenceId,
    ) -> Result<SecretString, WorkspaceRunFailure> {
        #[cfg(test)]
        if let Some(key) = &self.fixture {
            return Ok(key.clone());
        }
        GatewaySecretResolver::new()
            .resolve_string(catalog, id, SecretPurpose::ProviderApiKey)
            .await
            .map(|value| SecretString::from(value.expose_secret()))
            .map_err(|_| WorkspaceRunFailure::ModelUnavailable)
    }
}
