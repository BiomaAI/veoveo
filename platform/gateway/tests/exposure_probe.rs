use veoveo_gateway_contract::GatewayAction;
#[path = "../../../testing/fixtures/catalog_admission.rs"]
mod catalog_admission;
use std::{collections::BTreeSet, path::Path};

use veoveo_mcp_contract::{
    GatewayProfileId, LocalToolName, PolicyEffect, PolicyTarget, Principal, PrincipalKind,
    PromptName, ServerSlug, TokenIssuer, TokenSubject, TraceId,
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
        let profile_id = GatewayProfileId::parse(profile).unwrap();
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
        let profile_id = GatewayProfileId::parse(profile_name).unwrap();
        let profile = catalog.profile(&profile_id).expect("console profile");
        let principal = Principal {
            id: PrincipalId::parse("app-acceptance@example.com").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("https://idp.example.com").unwrap(),
            subject: TokenSubject::parse("app-acceptance").unwrap(),
            tenant: Some(TenantId::parse("enterprise").unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::from([
                RoleId::parse("operator").unwrap(),
                RoleId::parse("administrator").unwrap(),
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
                action: GatewayAction::ResourcesList.into(),
                target: &PolicyTarget::Resource {
                    server: ServerSlug::parse(server).unwrap(),
                    uri: ResourceUri::new(uri).unwrap(),
                },
                trace_id: &TraceId::parse(format!("{profile_name}-{server}-app")).unwrap(),
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
    let admin = GatewayProfileId::parse("admin").unwrap();
    let operator = GatewayProfileId::parse("operator").unwrap();
    for (kind, id, roles) in [
        (
            PrincipalKind::User,
            "app-acceptance@example.com",
            BTreeSet::from([RoleId::parse("administrator").unwrap()]),
        ),
        (
            PrincipalKind::Service,
            "https://veoveo.example/oauth#admin-service",
            BTreeSet::new(),
        ),
    ] {
        let mut principal = Principal {
            id: PrincipalId::parse(id).unwrap(),
            kind,
            issuer: TokenIssuer::parse("https://veoveo.example/oauth").unwrap(),
            subject: TokenSubject::parse("recording-acceptance").unwrap(),
            tenant: Some(TenantId::parse("enterprise").unwrap()),
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
            .remove(&ScopeName::parse("admin:manage").unwrap());
        assert_recording_permissions(
            &catalog,
            &missing_admin_scope,
            &admin,
            PolicyEffect::Deny,
            PolicyEffect::Deny,
        );

        let mut wrong_subject = principal.clone();
        wrong_subject.id = PrincipalId::parse("unregistered-service").unwrap();
        wrong_subject.roles.clear();
        assert_recording_permissions(
            &catalog,
            &wrong_subject,
            &admin,
            PolicyEffect::Deny,
            PolicyEffect::Deny,
        );

        principal.roles.insert(RoleId::parse("operator").unwrap());
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
    let server = ServerSlug::parse("recording").unwrap();
    let app = PolicyTarget::Resource {
        server: server.clone(),
        uri: ResourceUri::new("ui://recording/explorer.html").unwrap(),
    };
    let projection = PolicyTarget::Tool {
        server: server.clone(),
        tool: LocalToolName::parse("create_recording_projection").unwrap(),
    };
    let seal = PolicyTarget::Tool {
        server: server.clone(),
        tool: LocalToolName::parse("seal_recording").unwrap(),
    };
    let prompt = PolicyTarget::Prompt {
        server,
        prompt: PromptName::parse("recording_seal").unwrap(),
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
            action: action.into(),
            target,
            trace_id: &TraceId::parse("recording-permission-acceptance").unwrap(),
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
    let profile_id = GatewayProfileId::parse("operator").unwrap();
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

#[test]
fn bioma_initial_profile_allows_only_read_only_artifact_delivery() {
    use veoveo_mcp_contract::{
        CompletionExposure, DiscoveryFailureMode, Exposure, ResourceSelector, TaskExposure,
    };
    let catalog = GatewayCatalog::load_json(
        Path::new("../../examples/bioma/gateway.json"),
        catalog_admission::binding(),
    )
    .unwrap();
    let profile_id = GatewayProfileId::parse("operator-initial").unwrap();
    let profile = catalog.profile(&profile_id).unwrap();
    assert_eq!(
        profile.discovery_failure_mode,
        DiscoveryFailureMode::FailClosed
    );
    assert!(profile.artifact_upload.is_none());
    let mut tool_servers = profile
        .servers
        .iter()
        .filter(|server| !matches!(server.tools, Exposure::None))
        .map(|server| server.server.as_str())
        .collect::<Vec<_>>();
    tool_servers.sort();
    assert_eq!(tool_servers, ["duckdb", "frames", "media", "timeseries"]);
    assert_eq!(profile.servers.len(), 5);
    let artifact_server = ServerSlug::parse("artifact").unwrap();
    let (_, exposure, _) = catalog
        .profile_server(&profile_id, &artifact_server)
        .unwrap();
    assert!(matches!(exposure.tools, Exposure::None));
    assert!(matches!(exposure.prompts, Exposure::None));
    assert_eq!(exposure.completions, CompletionExposure::Disabled);
    assert_eq!(exposure.tasks, TaskExposure::Disabled);
    let uri = ResourceUri::new("artifact://01960000-0000-7000-8000-000000000001").unwrap();
    assert_eq!(
        catalog
            .server_for_resource_uri(&profile_id, uri.as_str())
            .unwrap()
            .1
            .slug,
        artifact_server
    );
    let Exposure::Listed(selectors) = &exposure.resources else {
        panic!("initial Artifact exposure must select only its resource scheme")
    };
    assert_eq!(selectors.len(), 1);
    assert!(
        matches!(&selectors[0], ResourceSelector::Scheme { scheme } if scheme.as_str() == "artifact")
    );
    let target = PolicyTarget::Artifact {
        server: artifact_server.clone(),
        artifact_uri: uri,
    };
    let trace = TraceId::parse("initial-artifact-policy").unwrap();
    for (id, kind, roles) in [
        (
            "https://veoveo.bioma.ai/oauth#operator-service",
            PrincipalKind::Service,
            BTreeSet::new(),
        ),
        (
            "initial-operator@example.com",
            PrincipalKind::User,
            BTreeSet::from([RoleId::parse("operator").unwrap()]),
        ),
    ] {
        let mut principal = Principal {
            id: PrincipalId::parse(id).unwrap(),
            kind,
            issuer: TokenIssuer::parse("https://veoveo.bioma.ai/oauth").unwrap(),
            subject: TokenSubject::parse("initial-artifact-reader").unwrap(),
            tenant: Some(TenantId::parse("bioma").unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles,
            scopes: profile.required_scopes.iter().cloned().collect(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };
        let decide = |principal: &Principal, action: GatewayAction, target: &PolicyTarget| {
            catalog
                .decide(PolicyRequest {
                    principal,
                    profile: &profile_id,
                    action: action.into(),
                    target,
                    trace_id: &trace,
                })
                .effect
        };
        assert_eq!(
            decide(&principal, GatewayAction::ArtifactRead, &target),
            PolicyEffect::Allow
        );
        let metadata = PolicyTarget::Resource {
            server: artifact_server.clone(),
            uri: ResourceUri::new("artifact://metadata/01960000-0000-7000-8000-000000000001")
                .unwrap(),
        };
        let occurrence = PolicyTarget::Resource {
            server: artifact_server.clone(),
            uri: ResourceUri::new("artifact://01960000-0000-7000-8000-000000000001").unwrap(),
        };
        let template = PolicyTarget::ResourceTemplate {
            server: artifact_server.clone(),
            uri: veoveo_types::ResourceTemplateUri::new("artifact://metadata/{artifact_id}")
                .unwrap(),
        };
        let occurrence_template = PolicyTarget::ResourceTemplate {
            server: artifact_server.clone(),
            uri: veoveo_types::ResourceTemplateUri::new("artifact://{artifact_id}").unwrap(),
        };
        for (action, selected) in [
            (GatewayAction::ResourcesRead, &metadata),
            (GatewayAction::ResourcesRead, &occurrence),
            (GatewayAction::ResourcesTemplatesList, &template),
            (GatewayAction::ResourcesTemplatesList, &occurrence_template),
        ] {
            assert_eq!(decide(&principal, action, selected), PolicyEffect::Allow);
        }
        for action in [
            GatewayAction::ArtifactUpload,
            GatewayAction::ToolsList,
            GatewayAction::ToolsCall,
            GatewayAction::TasksGet,
            GatewayAction::TasksUpdate,
            GatewayAction::TasksCancel,
            GatewayAction::PromptsList,
            GatewayAction::PromptsGet,
            GatewayAction::CompletionComplete,
            GatewayAction::SubscriptionsListen,
        ] {
            assert_eq!(decide(&principal, action, &target), PolicyEffect::Deny);
        }
        let other_server = PolicyTarget::Resource {
            server: ServerSlug::parse("frames").unwrap(),
            uri: ResourceUri::new("artifact://metadata/01960000-0000-7000-8000-000000000001")
                .unwrap(),
        };
        assert_eq!(
            decide(&principal, GatewayAction::ResourcesRead, &other_server),
            PolicyEffect::Deny
        );
        let ui = PolicyTarget::Resource {
            server: artifact_server.clone(),
            uri: ResourceUri::new("ui://artifact/library.html").unwrap(),
        };
        assert_eq!(
            decide(&principal, GatewayAction::ResourcesRead, &ui),
            PolicyEffect::Deny
        );
        let mutation = PolicyTarget::Tool {
            server: artifact_server.clone(),
            tool: LocalToolName::parse("grant_access").unwrap(),
        };
        assert_eq!(
            decide(&principal, GatewayAction::ToolsCall, &mutation),
            PolicyEffect::Deny
        );
        principal.scopes.clear();
        for (action, selected) in [
            (GatewayAction::ArtifactRead, &target),
            (GatewayAction::ResourcesRead, &metadata),
            (GatewayAction::ResourcesTemplatesList, &template),
            (GatewayAction::ResourcesTemplatesList, &occurrence_template),
        ] {
            assert_eq!(decide(&principal, action, selected), PolicyEffect::Deny);
        }
        principal.scopes = profile.required_scopes.iter().cloned().collect();
        principal.id = PrincipalId::parse("https://veoveo.bioma.ai/oauth#foreign-service").unwrap();
        principal.roles.clear();
        for (action, selected) in [
            (GatewayAction::ArtifactRead, &target),
            (GatewayAction::ResourcesRead, &metadata),
            (GatewayAction::ResourcesTemplatesList, &template),
            (GatewayAction::ResourcesTemplatesList, &occurrence_template),
        ] {
            assert_eq!(decide(&principal, action, selected), PolicyEffect::Deny);
        }
    }
}
