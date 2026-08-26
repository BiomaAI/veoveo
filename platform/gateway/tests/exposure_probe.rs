use std::{collections::BTreeSet, path::Path};

use veoveo_mcp_contract::{
    GatewayAction, GatewayProfileId, PolicyEffect, PolicyTarget, Principal, PrincipalId,
    PrincipalKind, ResourceUri, RoleId, ScopeName, ServerSlug, TenantId, TokenIssuer, TokenSubject,
    TraceId,
};
use veoveo_mcp_gateway::{GatewayCatalog, PolicyRequest, www_authenticate_challenge};

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
fn local_console_profiles_authorize_every_release_target_app_resource() {
    let apps = [
        ("artifact", "ui://artifact/library.html"),
        ("recording", "ui://recording/explorer.html"),
        ("optimization", "ui://optimization/routes.html"),
        ("optimization", "ui://optimization/models.html"),
        ("reason", "ui://reason/analyses.html"),
        ("media", "ui://media/studio.html"),
        ("duckdb", "ui://duckdb/workbench.html"),
        ("datasheet", "ui://datasheet/workbench.html"),
        ("frames", "ui://frames/workspace.html"),
        ("time", "ui://time/timeline.html"),
        ("charts", "ui://charts/composer.html"),
    ];
    let catalog =
        GatewayCatalog::load_json(Path::new(LOCAL_CONTROL_PLANE)).expect("load control plane");

    for profile_name in ["operator", "admin"] {
        let profile_id = GatewayProfileId::new(profile_name).unwrap();
        let profile = catalog.profile(&profile_id).expect("console profile");
        let principal = Principal {
            id: PrincipalId::new("app-acceptance@example.com").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            subject: TokenSubject::new("app-acceptance").unwrap(),
            tenant: Some(TenantId::new("enterprise").unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::from([
                RoleId::new("operator").unwrap(),
                RoleId::new("administrator").unwrap(),
            ]),
            scopes: profile.required_scopes.iter().cloned().collect(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };

        for (server, uri) in apps {
            let owner = catalog
                .server_for_resource_uri(&profile_id, uri)
                .map(|(_, manifest)| manifest.slug.to_string());
            assert_eq!(
                owner.as_deref(),
                Some(server),
                "{uri} is not projected through the {profile_name} profile"
            );

            let decision = catalog.decide(PolicyRequest {
                principal: &principal,
                profile: &profile_id,
                action: GatewayAction::ResourcesList,
                target: &PolicyTarget::Resource {
                    server: ServerSlug::new(server).unwrap(),
                    uri: ResourceUri::new(uri).unwrap(),
                },
                trace_id: &TraceId::new(format!("{profile_name}-{server}-app")).unwrap(),
            });
            assert_eq!(
                decision.effect,
                PolicyEffect::Allow,
                "{uri} is projected but policy denied it for {profile_name}: {decision:?}"
            );
        }
    }
}

#[test]
fn local_operator_profile_challenges_for_the_complete_view_scope_bundle() {
    let expected_scopes = [
        "operator:use",
        "uav-sim:control",
        "uav-sim:stream",
        "view:read",
        "view:write",
        "view:capture",
        "map:dataset:read",
        "map:route",
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
        "Bearer resource_metadata=\"https://veoveo.example/.well-known/oauth-protected-resource/mcp/operator\", scope=\"operator:use uav-sim:control uav-sim:stream view:read view:write view:capture map:dataset:read map:route time:read\"",
        "operator authorization challenge"
    );

    let metadata = catalog
        .protected_resource_metadata(&profile_id)
        .expect("operator protected-resource metadata");
    assert!(
        metadata
            .scopes_supported
            .iter()
            .any(|scope| scope == "uav-sim:read"),
        "operator protected-resource metadata omits UAV domain read authority"
    );
}
