#[path = "../../../testing/fixtures/catalog_registry.rs"]
mod catalog_registry;
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
            let tool = LocalToolName::parse(name).unwrap();
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
            .validate(&catalog_registry::registry())
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
                .get("resourcesListChanged")
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

#[test]
fn installation_configuration_rejects_unknown_core_and_registered_section_fields() {
    let fixture: Value = serde_json::from_slice(
        &fs::read("../../examples/bioma/gateway.json").expect("read Bioma control plane"),
    )
    .unwrap();
    let registry = catalog_registry::registry();
    serde_json::from_value::<GatewayControlPlane>(fixture.clone())
        .unwrap()
        .validate(&registry)
        .unwrap();
    for pointer in [
        "",
        "/branding",
        "/identityProviders/0",
        "/identityProviders/0/jwks",
        "/identityProviders/0/claimMapping",
        "/identityProviders/0/claimMapping/tenant",
        "/authorizationServers/0",
        "/servers/0",
        "/servers/0/upstream",
        "/servers/0/capabilities",
        "/servers/0/ownedRoutes/0",
        "/profiles/0",
        "/profiles/0/servers/0",
        "/profiles/0/servers/0/tools",
        "/profiles/0/servers/0/prompts",
        "/tenants/0",
        "/policies/0",
        "/policies/0/rules/0",
        "/dataLabels/0",
        "/workContexts/0",
        "/workContexts/0/outputPolicy",
        "/workContexts/0/outputPolicy/initialGrants/0",
        "/workContexts/0/memberships/0",
        "/oauthClients/0",
        "/oidcClients/0",
        "/secrets/0",
        "/secrets/0/owner",
        "/recording_ingest_resources/0",
        "/recording_ingest_resources/0/upstream",
        "/recording_ingest_resources/0/producers/0",
        "/recording_ingest_resources/0/producers/0/quotas",
        "/recording_ingest_resources/0/producers/0/blueprints",
        "/recording_ingest_resources/0/producers/0/retention",
    ] {
        let mut invalid = fixture.clone();
        invalid
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("missing fixture object {pointer}"))
            .as_object_mut()
            .unwrap()
            .insert("misspelled_config_field".into(), Value::Bool(true));
        let registered_section = pointer.starts_with("/recording_ingest_resources/");
        if registered_section {
            let error =
                serde_json::from_value::<veoveo_recording_contract::RecordingCatalogSection>(
                    invalid["recording_ingest_resources"].clone(),
                )
                .expect_err("registered owner decoder must reject the unknown field");
            assert!(
                error.to_string().contains("misspelled_config_field"),
                "{pointer}: {error}"
            );
        }
        let outcome = serde_json::from_value::<GatewayControlPlane>(invalid)
            .map_err(|error| error.to_string())
            .and_then(|control| {
                control
                    .validate(&registry)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            });
        let error = outcome.expect_err(pointer);
        if pointer.is_empty() {
            assert_eq!(error, "unknown or unbound catalog section");
        } else if registered_section {
            assert_eq!(error, "extension admission failed");
        } else {
            assert!(
                error.contains("misspelled_config_field"),
                "{pointer}: {error}"
            );
        }
    }
}

#[test]
fn installation_metadata_and_claim_dictionaries_remain_extensible() {
    let mut fixture: Value =
        serde_json::from_slice(&fs::read("../../examples/bioma/gateway.json").unwrap()).unwrap();
    fixture["metadata"]["installation_note"] = serde_json::json!({"custom_field": true});
    fixture["servers"][0]["metadata"]["custom_field"] = Value::Bool(true);
    fixture["identityProviders"][0]["claimMapping"]["tenant"]["values"]["installation-defined-claim"] =
        Value::String("bioma".into());
    serde_json::from_value::<GatewayControlPlane>(fixture)
        .unwrap()
        .validate(&catalog_registry::registry())
        .unwrap();
}

#[test]
fn bioma_control_plane_revision_binds_admitted_serialization() {
    use sha2::{Digest, Sha256};
    let control: GatewayControlPlane =
        serde_json::from_slice(&fs::read("../../examples/bioma/gateway.json").unwrap()).unwrap();
    control.validate(&catalog_registry::registry()).unwrap();
    let digest = hex::encode(Sha256::digest(serde_json::to_vec(&control).unwrap()));
    if std::env::var_os("VEOVEO_CAPTURE_BIOMA_CONTROL_PLANE_DIGEST").is_some() {
        println!("BIOMA_CONTROL_PLANE_SHA256={digest}");
    } else {
        let values = fs::read_to_string("../../examples/bioma/values.yaml").unwrap();
        let configured = values
            .lines()
            .find_map(|line| line.trim().strip_prefix("controlPlaneRevision: "))
            .unwrap();
        assert_eq!(
            configured, digest,
            "installation revision must hash admitted compact JSON, not author-file bytes"
        );
    }
}
