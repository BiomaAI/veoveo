use rmcp::model::ErrorData as McpError;
use veoveo_mcp_contract::{CertificateAuthoritySource, SecretPurpose, ServerManifest};

use crate::{GatewayCatalog, GatewaySecretResolver, mcp_support::mcp_internal};

const UPSTREAM_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub(super) async fn build_upstream_http_client(
    catalog: &GatewayCatalog,
    server: &ServerManifest,
) -> Result<reqwest::Client, McpError> {
    let mut builder = reqwest::Client::builder()
        .timeout(UPSTREAM_HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());

    for trust_anchor in &server.upstream.trusted_certificate_authorities {
        match trust_anchor {
            CertificateAuthoritySource::File { path } => {
                let bytes = std::fs::read(path.as_str()).map_err(|err| {
                    mcp_internal(format!(
                        "failed to read upstream CA certificate `{path}` for server `{}`: {err}",
                        server.slug
                    ))
                })?;
                let certificates = reqwest::Certificate::from_pem_bundle(&bytes).map_err(|err| {
                    mcp_internal(format!(
                        "failed to parse upstream CA certificate `{path}` for server `{}`: {err}",
                        server.slug
                    ))
                })?;
                for certificate in certificates {
                    builder = builder.add_root_certificate(certificate);
                }
            }
        }
    }

    if let (Some(certificate_id), Some(private_key_id)) = (
        server.upstream.client_certificate.as_ref(),
        server.upstream.client_private_key.as_ref(),
    ) {
        let resolver = GatewaySecretResolver::new();
        let certificate = resolver
            .resolve_string(catalog, certificate_id, SecretPurpose::TlsClientCertificate)
            .await
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to resolve upstream TLS client certificate for server `{}`: {err}",
                    server.slug
                ))
            })?;
        let private_key = resolver
            .resolve_string(catalog, private_key_id, SecretPurpose::TlsClientPrivateKey)
            .await
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to resolve upstream TLS client private key for server `{}`: {err}",
                    server.slug
                ))
            })?;
        let identity_pem = format!(
            "{}\n{}",
            certificate.expose_secret(),
            private_key.expose_secret()
        );
        let identity = reqwest::Identity::from_pem(identity_pem.as_bytes()).map_err(|err| {
            mcp_internal(format!(
                "failed to parse upstream TLS client identity for server `{}`: {err}",
                server.slug
            ))
        })?;
        builder = builder.identity(identity);
    }

    builder
        .build()
        .map_err(|err| mcp_internal(format!("failed to build upstream HTTP client: {err}")))
}
