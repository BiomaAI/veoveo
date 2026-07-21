use std::path::Path;

use veoveo_mcp_contract::{GatewayProfileId, ScopeName};
use veoveo_mcp_gateway::{GatewayCatalog, www_authenticate_challenge};

const SHIPPED_CONTROL_PLANES: [&str; 2] = [
    "../../configs/gateway.local.json",
    "../../examples/bioma/gateway.json",
];

/// The preview app pages must stay exposed through every console-facing
/// profile in the shipped control planes; a missing `resource_projection`
/// or profile `ui://` projection item silently hides the app.
#[test]
fn shipped_control_planes_expose_the_view_preview_app() {
    for config in SHIPPED_CONTROL_PLANES {
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

#[test]
fn shipped_operator_profiles_challenge_for_the_complete_view_scope_bundle() {
    let expected_scopes = [
        "operator:use",
        "view:read",
        "view:write",
        "view:capture",
        "map:dataset:read",
        "time:read",
    ];

    for config in SHIPPED_CONTROL_PLANES {
        let catalog = GatewayCatalog::load_json(Path::new(config)).expect("load control plane");
        let profile_id = GatewayProfileId::new("operator").unwrap();
        let profile = catalog.profile(&profile_id).expect("operator profile");
        let scopes = profile
            .required_scopes
            .iter()
            .map(ScopeName::as_str)
            .collect::<Vec<_>>();

        assert_eq!(scopes, expected_scopes, "{config}: operator scope bundle");

        let challenge = www_authenticate_challenge(
            "https://veoveo.example/.well-known/oauth-protected-resource/mcp/operator",
            &profile.required_scopes,
        );
        assert_eq!(
            challenge,
            "Bearer resource_metadata=\"https://veoveo.example/.well-known/oauth-protected-resource/mcp/operator\", scope=\"operator:use view:read view:write view:capture map:dataset:read time:read\"",
            "{config}: operator authorization challenge"
        );
    }
}
