use serde_json::{Value, json};
use veoveo_mcp_contract::GatewayControlPlane;
use veoveo_mcp_gateway::GatewayCatalog;
pub(crate) fn fixture_catalog() -> GatewayCatalog {
    let mut value: Value =
        serde_json::from_str(include_str!("../../configs/gateway.smoke.json")).unwrap();
    value["tenants"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"test","title":"Test","metadata":{}}));
    value["work_contexts"].as_array_mut().unwrap().push(json!({
        "id":"shared", "tenant":"test", "title":"Shared", "policy_revision":"2026-07-02",
        "output_policy":{"owner":{"kind":"group","id":"collaborators"}},
        "memberships":[{"level":"contributor","groups":["collaborators"]}]
    }));
    value["policies"][0]["rules"].as_array_mut().unwrap().push(json!({
        "id":"agent-authoring-fixture", "effect":"allow", "profiles":["operator"],
        "required_scopes":["operator:use"],
        "actions":["agent_definitions_read","agent_definitions_read_content","agent_definitions_create",
            "agent_definitions_edit","agent_definitions_publish","agent_definitions_use",
            "agent_definitions_control","agent_definitions_archive","agent_definitions_transfer",
            "agent_instances_deploy","agent_instances_control"]
    }));
    GatewayCatalog::from_control_plane(
        serde_json::from_value::<GatewayControlPlane>(value).unwrap(),
        super::catalog_fixture::binding(),
    )
    .unwrap()
}
