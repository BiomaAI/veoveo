use std::{fs, path::Path};

use serde_json::Value;
use veoveo_mcp_contract::{GatewayControlPlane, JwksSource, LocalToolName};

#[test]
fn bioma_service_clients_use_distinct_installation_public_keys() {
    use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet};
    use sha2::{Digest, Sha256};
    let control: GatewayControlPlane =
        serde_json::from_slice(&fs::read("../../examples/bioma/gateway.json").unwrap()).unwrap();
    let mut moduli = std::collections::BTreeSet::new();
    for (client_id, file) in [
        ("operator-service", "operator-client-jwks.json"),
        ("admin-service", "admin-client-jwks.json"),
    ] {
        let client = control
            .oauth_clients
            .iter()
            .find(|c| c.id.as_str() == client_id)
            .unwrap();
        let Some(JwksSource::File { path }) = &client.jwks else {
            panic!("installation-owned public key required")
        };
        assert_eq!(path.as_str(), format!("/etc/veoveo/gateway/{file}"));
        let bytes = fs::read(format!("../../examples/bioma/{file}")).unwrap();
        let jwks: JwkSet = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(jwks.keys.len(), 1);
        let key = &jwks.keys[0];
        assert_ne!(key.common.key_id.as_deref(), Some("test-key"));
        let AlgorithmParameters::RSA(rsa) = &key.algorithm else {
            panic!("RS256 client key required")
        };
        assert_ne!(
            hex::encode(Sha256::digest(rsa.n.as_bytes())),
            "dbfe4bd5b8db300eb06c46c27d8886b4ef8f64f557620b65747ac07196e3c281",
            "public conformance key must never authenticate an installed client"
        );
        assert!(
            moduli.insert(rsa.n.clone()),
            "clients must not share private authority"
        );
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        for private in ["d", "p", "q", "dp", "dq", "qi", "oth"] {
            assert!(
                raw["keys"][0].get(private).is_none(),
                "private key material in public configuration"
            );
        }
    }
}

const CORE_CONTROL_PLANES: [&str; 3] = [
    "../../configs/gateway.local.json",
    "../../configs/gateway.smoke.json",
    "../../examples/bioma/gateway.json",
];

#[test]
fn bioma_profiles_expose_computer_execution_and_retained_maintenance() {
    let control: GatewayControlPlane = serde_json::from_slice(
        &fs::read("../../examples/bioma/gateway.json").expect("read Bioma control plane"),
    )
    .expect("decode Bioma control plane");
    for id in ["operator", "agent", "admin"] {
        let profile = control
            .profiles
            .iter()
            .find(|p| p.id.as_str() == id)
            .unwrap();
        let computers = profile
            .servers
            .iter()
            .find(|s| s.server.as_str() == "computers")
            .unwrap();
        for name in [
            "execute",
            "grant_automation",
            "revoke_automation",
            "update_template",
            "resume_update",
        ] {
            let tool = LocalToolName::new(name).unwrap();
            assert!(
                veoveo_policy::exposure_contains(&computers.tools, &tool),
                "Bioma {id} hides delivered Computer workflow {name}"
            );
        }
    }
}

#[test]
fn core_control_planes_satisfy_the_gateway_contract() {
    for path in CORE_CONTROL_PLANES {
        let bytes = fs::read(path).expect("read core control plane");
        serde_json::from_slice::<GatewayControlPlane>(&bytes)
            .expect("decode core control plane")
            .validate()
            .unwrap_or_else(|error| panic!("{path}: {error}"));
    }
}

#[test]
fn core_control_planes_use_exact_list_change_capabilities() {
    for path in CORE_CONTROL_PLANES {
        let text = fs::read_to_string(Path::new(path)).expect("read core control plane");
        assert!(
            !text.contains("\"notifications\""),
            "{path} still uses the generic notification capability"
        );
        let value: Value = serde_json::from_str(&text).expect("decode core control plane");
        for server in value["servers"].as_array().expect("servers array") {
            let capabilities = server["capabilities"]
                .as_object()
                .expect("capability object");
            if capabilities
                .get("resources_list_changed")
                .and_then(Value::as_bool)
                == Some(true)
            {
                assert_eq!(
                    capabilities.get("resources").and_then(Value::as_bool),
                    Some(true),
                    "{} claims resource list changes without resources",
                    server["slug"]
                );
            }
        }
    }
}
