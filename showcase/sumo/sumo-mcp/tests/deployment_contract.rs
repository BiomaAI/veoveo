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
        .validate(&veoveo_gateway_catalog::registry().unwrap())
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

#[test]
fn sumo_public_contract_preserves_schema_and_task_wire_profile() {
    use veoveo_sumo_mcp::contract::{SumoTaskKind, Vehicle};
    use veoveo_types::TaskTypeDefinition;
    let schema = serde_json::to_value(schemars::schema_for!(Vehicle)).unwrap();
    let properties = schema["properties"].as_object().unwrap();
    for field in ["speed_mps", "edge_id", "heading_degrees", "vehicle_class"] {
        assert!(
            properties.contains_key(field),
            "current SUMO wire field missing: {field}"
        );
    }
    assert_eq!(schema["additionalProperties"], false);
    let wire = serde_json::json!({"id":"vehicle","latitude":1.0,"longitude":2.0,"speed_mps":3.0,"edge_id":"edge","heading_degrees":4.0,"x_m":5.0,"y_m":6.0,"length_m":7.0,"width_m":8.0,"height_m":9.0,"vehicle_class":"passenger"});
    let names: std::collections::BTreeSet<_> = wire
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        properties
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        names
    );
    assert_eq!(
        schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        names
    );
    let vehicle: Vehicle = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(vehicle).unwrap(), wire);
    let mut unknown = wire;
    unknown["speedMps"] = serde_json::json!(3.0);
    assert!(serde_json::from_value::<Vehicle>(unknown).is_err());
    assert_eq!(SumoTaskKind::RunBatch.name().as_str(), "run_batch");
}
