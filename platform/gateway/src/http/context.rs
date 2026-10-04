use crate::{GatewayCatalog, GatewayCatalogHandle, GatewayState, GatewayUpstreamHttpClientPool};
use parking_lot::RwLock;
use std::sync::Arc;
use veoveo_gateway_contract::CertificateAuthoritySource;
use veoveo_mcp_contract::{GatewayInternalTokenIssuer, PublicDeployment};

pub type SharedHttpClient = Arc<RwLock<reqwest::Client>>;

#[derive(Clone)]
pub struct GatewayHttpContext {
    pub deployment: PublicDeployment,
    pub catalog: GatewayCatalogHandle,
    pub gateway_state: GatewayState,
    pub internal_token_issuer: GatewayInternalTokenIssuer,
    pub upstream_http: GatewayUpstreamHttpClientPool,
    pub auth_http: SharedHttpClient,
}

#[derive(Clone)]
pub struct ProfileAuthState {
    pub catalog: GatewayCatalogHandle,
    pub gateway_state: GatewayState,
    pub deployment: PublicDeployment,
    pub auth_http: SharedHttpClient,
}
impl From<&GatewayHttpContext> for ProfileAuthState {
    fn from(context: &GatewayHttpContext) -> Self {
        Self {
            catalog: context.catalog.clone(),
            gateway_state: context.gateway_state.clone(),
            deployment: context.deployment.clone(),
            auth_http: context.auth_http.clone(),
        }
    }
}

use anyhow::Context;
use veoveo_mcp_contract::ResourceAuthorizationServer;
const GATEWAY_AUTH_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub fn current_catalog(catalog: &crate::GatewayCatalogHandle) -> Arc<GatewayCatalog> {
    catalog.current()
}

pub fn current_http_client(http: &SharedHttpClient) -> reqwest::Client {
    http.read().clone()
}

pub fn public_oauth_issuer(public_base_url: &str) -> String {
    format!("{}/oauth", public_base_url.trim_end_matches('/'))
}

pub fn public_authorization_server<'a>(
    catalog: &'a GatewayCatalog,
    public_base_url: &str,
) -> Option<&'a ResourceAuthorizationServer> {
    catalog.authorization_server_by_issuer(&public_oauth_issuer(public_base_url))
}

pub fn build_http_client(catalog: &GatewayCatalog) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(GATEWAY_AUTH_HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());

    for identity_provider in catalog.identity_providers() {
        for trust_anchor in &identity_provider.trusted_certificate_authorities {
            match trust_anchor {
                CertificateAuthoritySource::File { path } => {
                    let bytes = std::fs::read(path.as_str()).with_context(|| {
                        format!(
                            "failed to read trusted CA certificate `{path}` for identity provider `{}`",
                            identity_provider.id
                        )
                    })?;
                    let certificate = reqwest::Certificate::from_pem(&bytes).with_context(|| {
                        format!(
                            "failed to parse trusted CA certificate `{path}` for identity provider `{}`",
                            identity_provider.id
                        )
                    })?;
                    builder = builder.add_root_certificate(certificate);
                }
            }
        }
    }

    builder
        .build()
        .context("failed to build gateway HTTP client")
}
