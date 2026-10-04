#[path = "../../../testing/fixtures/catalog_admission.rs"]
mod catalog_admission;
use std::{collections::BTreeSet, path::Path};

use veoveo_mcp_contract::{
    GatewayAction, GatewayProfileId, LocalToolName, PolicyEffect, PolicyTarget, Principal,
    PrincipalKind, PromptName, ServerSlug, TokenIssuer, TokenSubject, TraceId,
};
use veoveo_mcp_gateway::{GatewayCatalog, PolicyRequest, www_authenticate_challenge};
use veoveo_recording_contract::RecordingScope;
use veoveo_types::{PrincipalId, ResourceUri, RoleId, ScopeDefinition, ScopeName, TenantId};

const LOCAL_CONTROL_PLANE: &str = "../../configs/gateway.local.json";

/// The preview app pages must stay exposed through every local console-facing
/// profile; a missing `resource_projection`
/// or profile `ui://` projection item silently hides the app.
#[test]
fn local_control_plane_exposes_the_view_preview_app() {
    let catalog =
        GatewayCatalog::load_json(Path::new(LOCAL_CONTROL_PLANE), catalog_admission::binding())
            .expect("load control plane");
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
        GatewayCatalog::load_json(Path::new(LOCAL_CONTROL_PLANE), catalog_admission::binding())
            .expect("load control plane");

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
fn local_recording_reads_and_sealing_have_separate_permissions() {
    let catalog =
        GatewayCatalog::load_json(Path::new(LOCAL_CONTROL_PLANE), catalog_admission::binding())
            .expect("load control plane");
    let admin = GatewayProfileId::new("admin").unwrap();
    let operator = GatewayProfileId::new("operator").unwrap();
    for (kind, id, roles) in [
        (
            PrincipalKind::User,
            "app-acceptance@example.com",
            BTreeSet::from([RoleId::new("administrator").unwrap()]),
        ),
        (
            PrincipalKind::Service,
            "https://veoveo.example/oauth#admin-service",
            BTreeSet::new(),
        ),
    ] {
        let mut principal = Principal {
            id: PrincipalId::new(id).unwrap(),
            kind,
            issuer: TokenIssuer::new("https://veoveo.example/oauth").unwrap(),
            subject: TokenSubject::new("recording-acceptance").unwrap(),
            tenant: Some(TenantId::new("enterprise").unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles,
            scopes: catalog
                .profile(&admin)
                .unwrap()
                .required_scopes
                .iter()
                .cloned()
                .collect(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };
        assert_recording_permissions(
            &catalog,
            &principal,
            &admin,
            PolicyEffect::Allow,
            PolicyEffect::Deny,
        );
        principal.scopes.insert(RecordingScope::Seal.name().clone());
        assert_recording_permissions(
            &catalog,
            &principal,
            &admin,
            PolicyEffect::Allow,
            PolicyEffect::Allow,
        );

        let mut missing_admin_scope = principal.clone();
        missing_admin_scope
            .scopes
            .remove(&ScopeName::new("admin:manage").unwrap());
        assert_recording_permissions(
            &catalog,
            &missing_admin_scope,
            &admin,
            PolicyEffect::Deny,
            PolicyEffect::Deny,
        );

        let mut wrong_subject = principal.clone();
        wrong_subject.id = PrincipalId::new("unregistered-service").unwrap();
        wrong_subject.roles.clear();
        assert_recording_permissions(
            &catalog,
            &wrong_subject,
            &admin,
            PolicyEffect::Deny,
            PolicyEffect::Deny,
        );

        principal.roles.insert(RoleId::new("operator").unwrap());
        principal.scopes.extend(
            catalog
                .profile(&operator)
                .unwrap()
                .required_scopes
                .iter()
                .cloned(),
        );
        assert_recording_permissions(
            &catalog,
            &principal,
            &operator,
            PolicyEffect::Allow,
            PolicyEffect::Deny,
        );
    }
}

fn assert_recording_permissions(
    catalog: &GatewayCatalog,
    principal: &Principal,
    profile: &GatewayProfileId,
    ordinary: PolicyEffect,
    sealing: PolicyEffect,
) {
    let server = ServerSlug::new("recording").unwrap();
    let app = PolicyTarget::Resource {
        server: server.clone(),
        uri: ResourceUri::new("ui://recording/explorer.html").unwrap(),
    };
    let projection = PolicyTarget::Tool {
        server: server.clone(),
        tool: LocalToolName::new("create_recording_projection").unwrap(),
    };
    let seal = PolicyTarget::Tool {
        server: server.clone(),
        tool: LocalToolName::new("seal_recording").unwrap(),
    };
    let prompt = PolicyTarget::Prompt {
        server,
        prompt: PromptName::new("recording-seal").unwrap(),
    };
    for (action, target, expected) in [
        (GatewayAction::ResourcesList, &app, ordinary),
        (GatewayAction::ResourcesRead, &app, ordinary),
        (GatewayAction::ToolsList, &projection, ordinary),
        (GatewayAction::ToolsCall, &projection, ordinary),
        (GatewayAction::ToolsList, &seal, sealing),
        (GatewayAction::ToolsCall, &seal, sealing),
        (GatewayAction::PromptsList, &prompt, sealing),
        (GatewayAction::PromptsGet, &prompt, sealing),
    ] {
        let decision = catalog.decide(PolicyRequest {
            principal,
            profile,
            action,
            target,
            trace_id: &TraceId::new("recording-permission-acceptance").unwrap(),
        });
        assert_eq!(decision.effect, expected, "{decision:?}");
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
        GatewayCatalog::load_json(Path::new(LOCAL_CONTROL_PLANE), catalog_admission::binding())
            .expect("load control plane");
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
