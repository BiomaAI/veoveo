use std::path::Path;

use veoveo_mcp_contract::GatewayProfileId;
use veoveo_mcp_gateway::GatewayCatalog;

/// The preview app pages must stay exposed through every console-facing
/// profile in the shipped control planes; a missing `resource_projection`
/// or profile `ui://` projection item silently hides the app.
#[test]
fn shipped_control_planes_expose_the_view_preview_app() {
    for config in ["../../configs/gateway.local.json", "../../examples/bioma/gateway.json"] {
        let catalog = GatewayCatalog::load_json(Path::new(config)).expect("load control plane");
        for profile in ["operator", "admin"] {
            let profile_id = GatewayProfileId::new(profile).unwrap();
            let owner = catalog
                .server_for_resource_uri(&profile_id, "ui://view/preview.html")
                .map(|(_, server)| server.slug.to_string());
            assert_eq!(
                owner.as_deref(),
                Some("view"),
                "{config}: ui://view/preview.html is not exposed for {profile}"
            );
        }
    }
}
