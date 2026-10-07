use crate::test_catalog_admission as catalog_admission;
use veoveo_gateway_contract::UpstreamUrl;
use veoveo_mcp_gateway::GatewayCatalog;

use rcgen::generate_simple_self_signed;
use serde_json::json;
use veoveo_mcp_contract::{GatewayControlPlane, ServerSlug};

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
        .server(&ServerSlug::parse("media").expect("server slug"))
        .expect("media server");

    ComputersGatewayClientPool::new()
        .client(&catalog, server)
        .await
        .expect("WebSocket uses the same mutual TLS identity and roots");

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
        .server(&ServerSlug::parse("media").expect("server slug"))
        .expect("media server");

    let err = ComputersGatewayClientPool::new()
        .client(&catalog, server)
        .await
        .expect_err("invalid mutual TLS material must fail closed");
    let message = format!("{err:?}");
    assert!(
        message.contains("failed to parse upstream TLS client identity"),
        "unexpected error: {message}"
    );
    assert!(
        ComputersGatewayClientPool::new()
            .client(&catalog, server)
            .await
            .is_err()
    );

    let _ = std::fs::remove_file(ca_path);
}

#[tokio::test]
async fn transport_equivalent_servers_share_one_http_client() {
    let control_plane: GatewayControlPlane =
        serde_json::from_str(SMOKE_CONTROL_PLANE).expect("smoke control plane json");
    let catalog = GatewayCatalog::from_control_plane(control_plane, catalog_admission::binding())
        .expect("validated catalog");
    let first = catalog
        .server(&ServerSlug::parse("media").expect("server slug"))
        .expect("media server");
    let mut second = first.clone();
    second.slug = ServerSlug::parse("second").expect("second server slug");
    second.upstream.url =
        UpstreamUrl::parse("http://127.0.0.1:8788/second/mcp").expect("second upstream URL");
    let pool = ComputersGatewayClientPool::new();

    let (first_ws, second_ws) =
        tokio::join!(pool.client(&catalog, first), pool.client(&catalog, &second));
    first_ws.expect("first WebSocket pool");
    second_ws.expect("second WebSocket pool");
    assert_eq!(pool.clients.read().await.len(), 1);
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
    upstream["healthUrl"] = json!("https://media.internal/media/healthz");
    upstream["security"] = json!("mutual_tls");
    upstream["trustedCertificateAuthorities"] = json!([
        {
            "source": "file",
            "path": ca_path.to_string_lossy()
        }
    ]);
    upstream["clientCertificate"] = json!("media_upstream_tls_client_certificate");
    upstream["clientPrivateKey"] = json!("media_upstream_tls_client_private_key");

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
    GatewayCatalog::from_control_plane(control_plane, catalog_admission::binding())
        .expect("validated catalog")
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

#[tokio::test]
async fn catalog_revision_change_retires_previous_upgrade_clients() {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let plane: GatewayControlPlane = serde_json::from_str(SMOKE_CONTROL_PLANE).unwrap();
        let first = GatewayCatalog::from_control_plane(plane.clone(), catalog_admission::binding())
            .unwrap();
        let pool = ComputersGatewayClientPool::new();
        let slug = ServerSlug::parse("media").unwrap();
        pool.client(&first, first.server(&slug).unwrap())
            .await
            .unwrap();
        let mut changed = plane;
        changed.metadata = json!({"cache-test-revision": 2});
        let second = GatewayCatalog::from_control_plane(changed, first.admission()).unwrap();
        assert_ne!(first.configuration_sha256(), second.configuration_sha256());
        pool.client(&second, second.server(&slug).unwrap())
            .await
            .unwrap();
        let clients = pool.clients.read().await;
        assert_eq!(clients.len(), 1);
        assert!(
            clients
                .keys()
                .all(|key| key.catalog_revision() == second.configuration_sha256())
        );
    })
    .await
    .expect("upgrade cache retirement exceeded 10 seconds");
}
