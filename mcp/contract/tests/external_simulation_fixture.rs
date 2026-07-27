use std::path::Path;

use serde_json::json;
use veoveo_mcp_contract::{
    GatewayAction, GatewayBinding, GatewayControlPlane, GatewayServerFragment, ResourceScheme,
    compose_gateway_control_plane,
};

#[test]
fn external_simulation_fragment_composes_with_installation_owned_authority() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let base = serde_json::from_slice::<GatewayControlPlane>(
        &std::fs::read(repository.join("configs/gateway.local.json"))
            .expect("read base control plane"),
    )
    .expect("decode base control plane");
    let fragment = serde_json::from_slice::<GatewayServerFragment>(
        &std::fs::read(
            repository.join("testing/fixtures/external-simulation-extension/gateway-fragment.json"),
        )
        .expect("read external simulation fragment"),
    )
    .expect("decode external simulation fragment");
    let binding = serde_json::from_slice::<GatewayBinding>(
        &std::fs::read(
            repository
                .join("testing/fixtures/external-simulation-installation/gateway-binding.json"),
        )
        .expect("read external simulation binding"),
    )
    .expect("decode external simulation binding");

    let composed = compose_gateway_control_plane(base, vec![fragment], vec![binding])
        .expect("compose gateway");
    composed
        .control_plane
        .validate()
        .expect("validate composed control plane");
    assert_eq!(
        serde_json::to_value(&composed.requirements).expect("serialize requirements"),
        json!({
            "platformCapabilities": ["artifact", "frames", "simulation_view"],
            "artifactAudiences": ["anonymous-simulation"]
        })
    );
    assert!(
        composed
            .control_plane
            .servers
            .iter()
            .any(|server| server.slug.as_str() == "anonymous-simulation")
    );
    let anonymous = composed
        .control_plane
        .servers
        .iter()
        .find(|server| server.slug.as_str() == "anonymous-simulation")
        .expect("composed anonymous simulation server");
    assert_eq!(
        anonymous.referenced_resource_schemes,
        [
            ResourceScheme::new("artifact").expect("artifact scheme"),
            ResourceScheme::new("frames").expect("frames scheme"),
        ]
        .into_iter()
        .collect()
    );
    for rule_id in [
        "allow_simulation_view_surface_read",
        "allow_simulation_view_write_tools",
        "allow_simulation_view_streams",
    ] {
        let rule = composed
            .control_plane
            .policies
            .iter()
            .flat_map(|policy| &policy.rules)
            .find(|rule| rule.id.as_str() == rule_id)
            .unwrap_or_else(|| panic!("missing Simulation View policy rule `{rule_id}`"));
        assert!(
            rule.actions.contains(&GatewayAction::ToolsList),
            "Simulation View policy rule `{rule_id}` must expose its tools through tools/list"
        );
    }
    assert_eq!(composed.contributions.len(), 1);
}
