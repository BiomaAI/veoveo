use std::{fs, path::PathBuf};

use serde_json::Value;
use veoveo_mcp_contract::GatewayControlPlane;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .expect("SUMO MCP crate lives under <repository>/showcase/sumo")
        .to_owned()
}

#[test]
fn sumo_control_plane_satisfies_the_gateway_contract() {
    let path = repository_root().join("showcase/sumo/deploy/gateway.json");
    let text = fs::read_to_string(&path).expect("read SUMO control plane");
    serde_json::from_str::<GatewayControlPlane>(&text)
        .expect("decode SUMO control plane")
        .validate()
        .expect("validate SUMO control plane");

    assert!(
        !text.contains("\"notifications\""),
        "SUMO still uses the generic notification capability"
    );
    let value: Value = serde_json::from_str(&text).expect("decode SUMO control plane as JSON");
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
