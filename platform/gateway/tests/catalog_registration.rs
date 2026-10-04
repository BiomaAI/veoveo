#[path = "../../../testing/fixtures/catalog_admission.rs"]
mod catalog_admission;
use veoveo_mcp_contract::{GatewayControlPlane, PolicyDecision, PolicyRule};
use veoveo_mcp_gateway::GatewayCatalog;
use veoveo_policy::PolicyCatalog;

#[test]
fn full_catalog_readers_and_published_schemas_share_registration() {
    let registry = catalog_admission::registry();
    let text = include_str!("../../../configs/gateway.smoke.json");
    let plane: GatewayControlPlane = serde_json::from_str(text).unwrap();
    let gateway =
        GatewayCatalog::from_control_plane(plane.clone(), catalog_admission::binding()).unwrap();
    let standalone = PolicyCatalog::new(plane.clone(), registry.clone()).unwrap();
    assert_eq!(gateway.control_plane(), standalone.control_plane());
    assert_eq!(registry.actions().names().count(), 38);
    let schema = veoveo_mcp_contract::composed_gateway_schema::<GatewayControlPlane>(&registry);
    let validator = jsonschema::validator_for(schema.as_value()).unwrap();
    assert!(validator.is_valid(&serde_json::to_value(&plane).unwrap()));
    let mut unknown = plane.clone();
    unknown
        .extensions
        .insert("unknown_owner_section".into(), serde_json::json!({}));
    assert!(!validator.is_valid(&serde_json::to_value(&unknown).unwrap()));
    assert!(
        GatewayCatalog::from_control_plane(unknown.clone(), catalog_admission::binding()).is_err()
    );
    assert!(PolicyCatalog::new(unknown, registry.clone()).is_err());
    let mut unknown = plane.clone();
    unknown.policies[0].rules[0]
        .actions
        .insert(veoveo_types::ActionName::parse("unknown_action").unwrap());
    assert!(
        GatewayCatalog::from_control_plane(unknown.clone(), catalog_admission::binding()).is_err()
    );
    assert!(PolicyCatalog::new(unknown, registry.clone()).is_err());
    for schema in [
        veoveo_mcp_contract::composed_gateway_schema::<PolicyRule>(&registry),
        veoveo_mcp_contract::composed_gateway_schema::<PolicyDecision>(&registry),
    ] {
        let wire = schema.to_value();
        let actions = wire
            .pointer("/$defs/GatewayAction/enum")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        assert_eq!(actions.len(), 38);
    }
}
#[test]
fn raw_duplicates_and_typed_core_collision_fail_before_canonicalization() {
    let text = include_str!("../../../configs/gateway.smoke.json");
    let duplicate = format!("{{\"profiles\":[],{}", &text[text.find('{').unwrap() + 1..]);
    assert!(serde_json::from_str::<GatewayControlPlane>(&duplicate).is_err());
    let mut plane: GatewayControlPlane = serde_json::from_str(text).unwrap();
    plane
        .extensions
        .insert("servers".into(), serde_json::json!([]));
    assert!(GatewayCatalog::from_control_plane(plane, catalog_admission::binding()).is_err());
}
