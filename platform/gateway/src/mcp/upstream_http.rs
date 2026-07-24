use std::{collections::BTreeMap, sync::Arc};

use rmcp::model::ErrorData as McpError;
use sha2::{Digest, Sha256};
use tokio::sync::{OnceCell, RwLock};
use veoveo_mcp_contract::{CertificateAuthoritySource, SecretPurpose, ServerManifest};

use crate::{GatewayCatalog, GatewaySecretResolver, mcp_support::mcp_internal};

const UPSTREAM_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct UpstreamHttpClientKey {
    catalog_sha256: [u8; 32],
    configuration_sha256: [u8; 32],
}

/// Shared transport clients for gateway-to-server traffic.
///
/// MCP sessions retain their own protocol state and invocation authority, but
/// transport-equivalent upstreams share one reqwest connection pool and one
/// initialized TLS trust store for the active catalog revision.
#[derive(Debug, Clone, Default)]
pub struct GatewayUpstreamHttpClientPool {
    clients: Arc<RwLock<BTreeMap<UpstreamHttpClientKey, Arc<OnceCell<reqwest::Client>>>>>,
}

impl GatewayUpstreamHttpClientPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn client(
        &self,
        catalog: &GatewayCatalog,
        server: &ServerManifest,
    ) -> Result<reqwest::Client, McpError> {
        let key = upstream_http_client_key(catalog, server)?;
        let cell = {
            let mut clients = self.clients.write().await;
            clients
                .retain(|candidate, _| candidate.catalog_sha256 == catalog.configuration_sha256());
            clients
                .entry(key)
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone()
        };
        cell.get_or_try_init(|| build_upstream_http_client(catalog, server))
            .await
            .cloned()
    }

    #[cfg(test)]
    async fn entry_count(&self) -> usize {
        self.clients.read().await.len()
    }
}

fn upstream_http_client_key(
    catalog: &GatewayCatalog,
    server: &ServerManifest,
) -> Result<UpstreamHttpClientKey, McpError> {
    let configuration = serde_json::to_vec(&(
        server.upstream.security,
        &server.upstream.trusted_certificate_authorities,
        &server.upstream.client_certificate,
        &server.upstream.client_private_key,
    ))
    .map_err(|error| {
        mcp_internal(format!(
            "failed to fingerprint upstream HTTP client configuration: {error}"
        ))
    })?;
    Ok(UpstreamHttpClientKey {
        catalog_sha256: catalog.configuration_sha256(),
        configuration_sha256: Sha256::digest(configuration).into(),
    })
}

async fn build_upstream_http_client(
    catalog: &GatewayCatalog,
    server: &ServerManifest,
) -> Result<reqwest::Client, McpError> {
    let mut builder = reqwest::Client::builder()
        // Streamable HTTP keeps a GET/SSE response open for the lifetime of the
        // MCP session. A total request timeout would tear that stream down and
        // create notification gaps, so bound connection establishment only.
        .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
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

#[cfg(test)]
mod tests {
    use rcgen::generate_simple_self_signed;
    use serde_json::json;
    use veoveo_mcp_contract::{GatewayControlPlane, ServerSlug, UpstreamUrl};

    use super::*;

    const SMOKE_CONTROL_PLANE: &str = include_str!("../../../../configs/gateway.smoke.json");

    #[tokio::test]
    async fn builds_client_with_mutual_tls_material_from_typed_secrets() {
        let certified_key = generate_simple_self_signed(vec!["media.internal".to_string()])
            .expect("test certificate material");
        let cert_pem = certified_key.cert.pem();
        let key_pem = certified_key.signing_key.serialize_pem();
        let ca_path = write_temp_ca(&cert_pem);
        let cert_env = unique_env_name("CERT");
        let key_env = unique_env_name("KEY");

        set_test_env(&cert_env, &cert_pem);
        set_test_env(&key_env, &key_pem);

        let catalog = catalog_with_mutual_tls_upstream(&ca_path, &cert_env, &key_env);
        let server = catalog
            .server(&ServerSlug::new("media").expect("server slug"))
            .expect("media server");

        build_upstream_http_client(&catalog, server)
            .await
            .expect("mutual TLS upstream client");

        let _ = std::fs::remove_file(ca_path);
    }

    #[tokio::test]
    async fn rejects_invalid_mutual_tls_identity_material() {
        let certified_key = generate_simple_self_signed(vec!["media.internal".to_string()])
            .expect("test certificate material");
        let cert_pem = certified_key.cert.pem();
        let ca_path = write_temp_ca(&cert_pem);
        let cert_env = unique_env_name("CERT");
        let key_env = unique_env_name("KEY");

        set_test_env(&cert_env, &cert_pem);
        set_test_env(&key_env, "not a private key");

        let catalog = catalog_with_mutual_tls_upstream(&ca_path, &cert_env, &key_env);
        let server = catalog
            .server(&ServerSlug::new("media").expect("server slug"))
            .expect("media server");

        let err = build_upstream_http_client(&catalog, server)
            .await
            .expect_err("invalid mutual TLS material must fail closed");
        let message = format!("{err:?}");
        assert!(
            message.contains("failed to parse upstream TLS client identity"),
            "unexpected error: {message}"
        );

        let _ = std::fs::remove_file(ca_path);
    }

    #[tokio::test]
    async fn transport_equivalent_servers_share_one_http_client() {
        let control_plane: GatewayControlPlane =
            serde_json::from_str(SMOKE_CONTROL_PLANE).expect("smoke control plane json");
        let catalog = GatewayCatalog::from_control_plane(control_plane).expect("validated catalog");
        let first = catalog
            .server(&ServerSlug::new("media").expect("server slug"))
            .expect("media server");
        let mut second = first.clone();
        second.slug = ServerSlug::new("second").expect("second server slug");
        second.upstream.url =
            UpstreamUrl::new("http://127.0.0.1:8788/second/mcp").expect("second upstream URL");
        let pool = GatewayUpstreamHttpClientPool::new();

        let (first_client, second_client) =
            tokio::join!(pool.client(&catalog, first), pool.client(&catalog, &second));
        first_client.expect("first shared client");
        second_client.expect("second shared client");

        assert_eq!(pool.entry_count().await, 1);
    }

    fn catalog_with_mutual_tls_upstream(
        ca_path: &std::path::Path,
        cert_env: &str,
        key_env: &str,
    ) -> GatewayCatalog {
        let mut control_plane: serde_json::Value =
            serde_json::from_str(SMOKE_CONTROL_PLANE).expect("smoke control plane json");
        let upstream = &mut control_plane["servers"][0]["upstream"];
        upstream["url"] = json!("https://media.internal/media/mcp");
        upstream["security"] = json!("mutual_tls");
        upstream["trusted_certificate_authorities"] = json!([
            {
                "source": "file",
                "path": ca_path.to_string_lossy()
            }
        ]);
        upstream["client_certificate"] = json!("media_upstream_tls_client_certificate");
        upstream["client_private_key"] = json!("media_upstream_tls_client_private_key");

        control_plane["secrets"]
            .as_array_mut()
            .expect("secrets array")
            .extend([
                json!({
                    "id": "media_upstream_tls_client_certificate",
                    "source": "env",
                    "purpose": "tls_client_certificate",
                    "locator": cert_env,
                    "owner": {
                        "kind": "gateway"
                    }
                }),
                json!({
                    "id": "media_upstream_tls_client_private_key",
                    "source": "env",
                    "purpose": "tls_client_private_key",
                    "locator": key_env,
                    "owner": {
                        "kind": "gateway"
                    }
                }),
            ]);

        let control_plane: GatewayControlPlane =
            serde_json::from_value(control_plane).expect("typed control plane");
        GatewayCatalog::from_control_plane(control_plane).expect("validated catalog")
    }

    fn write_temp_ca(cert_pem: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("veoveo-upstream-ca-{}.pem", uuid::Uuid::new_v4()));
        std::fs::write(&path, cert_pem).expect("write CA certificate");
        path
    }

    fn unique_env_name(label: &str) -> String {
        format!("VEOVEO_TEST_UPSTREAM_TLS_{label}_{}", uuid::Uuid::new_v4()).replace('-', "_")
    }

    fn set_test_env(name: &str, value: &str) {
        // Rust 2024 marks process environment mutation unsafe because other threads may read it.
        // The tests use unique variable names and never mutate the same key twice.
        unsafe {
            std::env::set_var(name, value);
        }
    }
}
