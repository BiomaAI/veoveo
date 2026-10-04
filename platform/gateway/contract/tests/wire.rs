use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use veoveo_gateway_contract::*;

fn round_trip<T: JsonSchema + Serialize + DeserializeOwned>(wire: Value, title: &str) {
    let admitted: T = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(admitted).unwrap(), wire);
    let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
    assert_eq!(schema["title"], title);
    assert!(jsonschema::is_valid(&schema, &wire));
}

#[test]
fn existing_app_dependency_wire_and_schema_profiles() {
    round_trip::<AppResourceDependency>(
        json!({
            "app_resource": "ui://map/explorer", "server": "frames",
            "scheme": "frames", "uri_prefix": "frames://frame/",
            "required_scope": "frames:read", "operations": ["read", "subscribe"]
        }),
        "AppResourceDependency",
    );
    round_trip::<AppToolDependency>(
        json!({
            "app_resource": "ui://map/explorer", "server": "frames",
            "required_scope": "frames:read", "tools": [{"name":"read-frame", "target_tool":"read-frame"}]
        }),
        "AppToolDependency",
    );
    round_trip::<AppToolImport>(
        json!({"name":"read-frame", "target_tool":"read-frame"}),
        "AppToolImport",
    );
    round_trip::<AppResourceOperation>(json!("subscribe"), "AppResourceOperation");
    assert_eq!(
        APP_RESOURCE_DEPENDENCIES_META_KEY,
        "ai.veoveo/app-resource-dependencies"
    );
    assert_eq!(
        APP_TOOL_DEPENDENCIES_META_KEY,
        "ai.veoveo/app-tool-dependencies"
    );
}

#[test]
fn existing_discovery_wire_names_and_admission() {
    round_trip::<GatewayDiscoveryFailure>(
        json!({
            "server":"recording", "surface":"resource_templates", "code":"upstream_unavailable"
        }),
        "GatewayDiscoveryFailure",
    );
    round_trip::<GatewayDiscoverySurface>(json!("resource_templates"), "GatewayDiscoverySurface");
    round_trip::<GatewayDiscoveryFailureCode>(
        json!("discovery_pending"),
        "GatewayDiscoveryFailureCode",
    );
    assert!(
        serde_json::from_value::<GatewayDiscoveryFailure>(json!({
            "server":"invalid/slash", "surface":"resources", "code":"upstream_unavailable"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<AppToolImport>(
            json!({"name":"bad/name", "target_tool":"read-frame"})
        )
        .is_err()
    );
    assert_eq!(
        GATEWAY_DISCOVERY_DEGRADATION_META_KEY,
        "ai.veoveo/gateway-discovery-degradation"
    );
}
