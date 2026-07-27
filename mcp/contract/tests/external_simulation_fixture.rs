use std::path::Path;

use serde_json::json;
use veoveo_mcp_contract::{
    GatewayBinding, GatewayControlPlane, GatewayServerFragment, compose_gateway_control_plane,
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
    assert_eq!(composed.contributions.len(), 1);
}
