use std::path::Path;

use veoveo_mcp_contract::{GatewayProfileId, ScopeName};
use veoveo_mcp_gateway::{GatewayCatalog, www_authenticate_challenge};

const LOCAL_CONTROL_PLANE: &str = "../../configs/gateway.local.json";

/// The preview app pages must stay exposed through every local console-facing
/// profile; a missing `resource_projection`
/// or profile `ui://` projection item silently hides the app.
#[test]
fn local_control_plane_exposes_the_view_preview_app() {
    let catalog =
        GatewayCatalog::load_json(Path::new(LOCAL_CONTROL_PLANE)).expect("load control plane");
    for profile in ["operator", "admin"] {
        let profile_id = GatewayProfileId::new(profile).unwrap();
        let owner = catalog
            .server_for_resource_uri(&profile_id, "ui://view/preview.html")
            .map(|(_, server)| server.slug.to_string());
        assert_eq!(
            owner.as_deref(),
            Some("view"),
            "ui://view/preview.html is not exposed for {profile}"
        );
    }
}

#[test]
fn local_operator_profile_challenges_for_the_complete_view_scope_bundle() {
    let expected_scopes = [
        "operator:use",
        "simulation-view:read",
        "simulation-view:write",
        "simulation-view:stream",
        "view:read",
        "view:write",
        "view:capture",
        "map:dataset:read",
        "time:read",
    ];

    let catalog =
        GatewayCatalog::load_json(Path::new(LOCAL_CONTROL_PLANE)).expect("load control plane");
    let profile_id = GatewayProfileId::new("operator").unwrap();
    let profile = catalog.profile(&profile_id).expect("operator profile");
    let scopes = profile
        .required_scopes
        .iter()
        .map(ScopeName::as_str)
        .collect::<Vec<_>>();

    assert_eq!(scopes, expected_scopes, "operator scope bundle");

    let challenge = www_authenticate_challenge(
        "https://veoveo.example/.well-known/oauth-protected-resource/mcp/operator",
        &profile.required_scopes,
    );
    assert_eq!(
        challenge,
        "Bearer resource_metadata=\"https://veoveo.example/.well-known/oauth-protected-resource/mcp/operator\", scope=\"operator:use simulation-view:read simulation-view:write simulation-view:stream view:read view:write view:capture map:dataset:read time:read\"",
        "operator authorization challenge"
    );
}
