use std::{fs, path::PathBuf};

use serde_json::Value;
use veoveo_mcp_contract::GatewayControlPlane;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("smoke crate lives under <root>/crates")
        .to_owned()
}

fn load(path: &str) -> Value {
    serde_json::from_slice(&fs::read(repository_root().join(path)).expect("read control plane"))
        .expect("parse control plane")
}

fn normalize_bioma(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(normalize_bioma),
        Value::Object(values) => values.values_mut().for_each(normalize_bioma),
        Value::String(value) if value == "bioma" => *value = "enterprise".to_owned(),
        Value::String(value) => {
            *value = value.replace("https://veoveo.bioma.ai", "https://veoveo.example");
        }
        _ => {}
    }
}

fn canonical_client_shape(control_plane: &Value) -> Value {
    let mut clients = control_plane["oauth_clients"].clone();
    for client in clients.as_array_mut().expect("oauth_clients is an array") {
        let client = client.as_object_mut().expect("OAuth client is an object");
        client.remove("tenant");
        client.remove("jwks");
        client.remove("redirect_uris");
    }
    clients
}

#[test]
fn bioma_keeps_the_canonical_surface_and_only_overrides_deployment_identity() {
    let local = load("configs/gateway.local.json");
    let mut bioma = load("examples/bioma/gateway.json");
    serde_json::from_value::<GatewayControlPlane>(local.clone())
        .expect("local control plane contract")
        .validate()
        .expect("valid local control plane");
    serde_json::from_value::<GatewayControlPlane>(bioma.clone())
        .expect("Bioma control plane contract")
        .validate()
        .expect("valid Bioma control plane");

    let identity = &bioma["identity_providers"][0];
    let tenant_id = "e0ee3c6a-4f58-4f66-8de4-253226eeed5f";
    assert_eq!(identity["claim_mapping"]["subject"], "oid");
    assert_eq!(identity["claim_mapping"]["tenant"]["claim"], "tid");
    assert_eq!(
        identity["claim_mapping"]["tenant"]["values"][tenant_id],
        "bioma"
    );
    assert_eq!(
        identity["issuer"],
        format!("https://login.microsoftonline.com/{tenant_id}/v2.0")
    );
    assert_eq!(
        bioma["oidc_clients"][0]["redirect_uri"],
        "https://veoveo.bioma.ai/oauth/callback"
    );

    normalize_bioma(&mut bioma);
    for key in ["servers", "profiles", "policies", "data_labels", "secrets"] {
        assert_eq!(
            bioma[key], local[key],
            "Bioma `{key}` drifted from canonical"
        );
    }
    assert_eq!(
        canonical_client_shape(&bioma),
        canonical_client_shape(&local),
        "Bioma OAuth client capabilities drifted from canonical"
    );
    assert_eq!(
        bioma["metadata"]["environment"],
        local["metadata"]["environment"]
    );
}
