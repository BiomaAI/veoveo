//! Read-only composition checks; no services, credentials or network required.
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{
    GatewayControlPlane, PolicyEffect, PolicyTarget, Principal, PrincipalKind, ServerResourceUris,
};
use veoveo_mcp_gateway::{GatewayCatalog, PolicyRequest};
use veoveo_types::WorkContextMembershipLevel;

fn plane() -> GatewayControlPlane {
    serde_json::from_str(include_str!("../../gateway.json")).unwrap()
}

#[test]
fn indexer_discovers_approved_sources_without_write_or_cross_context_authority() {
    let plane = plane();
    plane
        .validate(&veoveo_gateway_catalog::registry().unwrap())
        .unwrap();
    let client = plane
        .oauth_clients
        .iter()
        .find(|c| c.knowledge_indexing.is_some())
        .unwrap();
    let profile = plane
        .profiles
        .iter()
        .find(|p| client.allowed_resources.contains(&p.protected_resource))
        .unwrap();
    let issuer = plane
        .authorization_servers
        .iter()
        .find(|s| s.id == client.authorization_server)
        .unwrap();
    let principal = Principal {
        id: format!("{}#{}", issuer.issuer, client.id).parse().unwrap(),
        kind: PrincipalKind::Service,
        issuer: issuer.issuer.clone(),
        subject: client.id.as_str().parse().unwrap(),
        tenant: client.tenant.clone(),
        scopes: client.allowed_scopes.clone(),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: None,
    };
    let catalog = GatewayCatalog::from_control_plane(
        plane.clone(),
        veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
            .bind(veoveo_gateway_catalog::registry().unwrap())
            .unwrap(),
    )
    .unwrap();
    let trace = "knowledge-reference".parse().unwrap();
    let decide = |action: GatewayAction, target: &PolicyTarget| {
        catalog
            .decide(PolicyRequest {
                principal: &principal,
                profile: &profile.id,
                action: action.into(),
                target,
                trace_id: &trace,
            })
            .effect
    };
    for action in [
        GatewayAction::ResourcesList,
        GatewayAction::ResourcesTemplatesList,
        GatewayAction::SubscriptionsListen,
    ] {
        assert_eq!(decide(action, &PolicyTarget::Gateway), PolicyEffect::Allow);
    }
    let knowledge = plane
        .servers
        .iter()
        .find(|s| s.slug.as_str() == "knowledge")
        .unwrap();
    for source in &plane.servers {
        if !source.knowledge.is_empty() {
            assert!(
                knowledge
                    .referenced_resource_schemes
                    .contains(&source.uri_scheme),
                "Knowledge must preserve owner addresses for approved source {}",
                source.slug
            );
        }
        let selected = client
            .knowledge_indexing
            .as_ref()
            .unwrap()
            .collections
            .iter()
            .any(|collection| collection.server() == &source.slug);
        let target = PolicyTarget::Resource {
            server: source.slug.clone(),
            uri: ServerResourceUris::new(source.uri_scheme.clone()).contract_uri(),
        };
        assert_eq!(
            decide(GatewayAction::ResourcesRead, &target),
            if selected {
                PolicyEffect::Allow
            } else {
                PolicyEffect::Deny
            },
            "{}",
            source.slug
        );
        for tool in &source.tools {
            assert_eq!(
                decide(
                    GatewayAction::ToolsCall,
                    &PolicyTarget::Tool {
                        server: source.slug.clone(),
                        tool: tool.clone()
                    }
                ),
                PolicyEffect::Deny
            );
        }
        assert!(
            source.knowledge.iter().all(|c| c.data_labels.is_empty()),
            "regulated content requires separate installation approval"
        );
    }
    for action in [
        GatewayAction::ToolsList,
        GatewayAction::ArtifactUpload,
        GatewayAction::AdminWrite,
        GatewayAction::TasksCancel,
    ] {
        assert_eq!(decide(action, &PolicyTarget::Gateway), PolicyEffect::Deny);
    }
    let deploy = catalog
        .registry()
        .action_key::<veoveo_agent_runtime::contract::AgentAction>()
        .unwrap()
        .action(veoveo_agent_runtime::contract::AgentAction::AgentInstancesDeploy)
        .unwrap();
    assert_eq!(
        catalog
            .decide(PolicyRequest {
                principal: &principal,
                profile: &profile.id,
                action: deploy.into(),
                target: &PolicyTarget::Gateway,
                trace_id: &trace,
            })
            .effect,
        PolicyEffect::Deny
    );
    for context in &plane.work_contexts {
        assert_eq!(
            context.membership_for(&principal, &client.id),
            (context.id == client.default_work_context)
                .then_some(WorkContextMembershipLevel::Viewer)
        );
    }
}

#[test]
fn user_profiles_can_find_knowledge_without_exposing_the_indexing_profile() {
    let catalog = GatewayCatalog::from_control_plane(
        plane(),
        veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
            .bind(veoveo_gateway_catalog::registry().unwrap())
            .unwrap(),
    )
    .unwrap();
    for name in ["operator", "admin", "workspace", "agent"] {
        let profile = name.parse().unwrap();
        assert_eq!(
            catalog
                .server_for_resource_uri(&profile, "knowledge://sources")
                .unwrap()
                .1
                .slug
                .as_str(),
            "knowledge"
        );
    }
    let plane = plane();
    let indexing = plane
        .profiles
        .iter()
        .find(|p| p.id.as_str() == "knowledge-indexing")
        .unwrap();
    for client in &plane.oauth_clients {
        assert_eq!(
            client
                .allowed_resources
                .contains(&indexing.protected_resource),
            client.knowledge_indexing.is_some()
        );
    }
}

fn ordinary_operator(
    plane: &GatewayControlPlane,
    scopes: std::collections::BTreeSet<veoveo_types::ScopeName>,
) -> Principal {
    let issuer = &plane
        .identity_providers
        .iter()
        .find(|idp| idp.id.as_str() == "enterprise")
        .unwrap()
        .issuer;
    Principal {
        id: format!("{issuer}#knowledge-acceptance-control")
            .parse()
            .unwrap(),
        kind: PrincipalKind::User,
        issuer: issuer.clone(),
        subject: "knowledge-acceptance-control".parse().unwrap(),
        tenant: Some("bioma".parse().unwrap()),
        roles: ["operator".parse().unwrap()].into(),
        scopes,
        groups: Default::default(),
        group_roles: Default::default(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: None,
    }
}

#[test]
fn isolated_knowledge_caller_admits_time_collections_and_refuses_owner_documents() {
    use std::collections::BTreeSet;
    let plane = plane();
    let catalog = GatewayCatalog::from_control_plane(
        plane.clone(),
        veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
            .bind(veoveo_gateway_catalog::registry().unwrap())
            .unwrap(),
    )
    .unwrap();
    let client = plane
        .oauth_clients
        .iter()
        .find(|c| c.id.as_str() == "knowledge-acceptance-public")
        .unwrap();
    let profile = plane
        .profiles
        .iter()
        .find(|p| p.id.as_str() == "knowledge-acceptance")
        .unwrap();
    let principal = ordinary_operator(&plane, client.allowed_scopes.clone());
    let trace = "knowledge-isolated-control".parse().unwrap();
    let decide = |actor: &Principal, action: GatewayAction, target: &PolicyTarget| {
        catalog
            .decide(PolicyRequest {
                principal: actor,
                profile: &profile.id,
                action: action.into(),
                target,
                trace_id: &trace,
            })
            .effect
    };
    let resource = |server: &str, uri: &str| PolicyTarget::Resource {
        server: server.parse().unwrap(),
        uri: veoveo_types::ResourceUri::new(uri).unwrap(),
    };
    let knowledge_selection = veoveo_policy::admit_resource_reads(
        &catalog,
        &principal,
        &profile.id,
        &"knowledge".parse().unwrap(),
    )
    .unwrap();
    for uri in [
        "knowledge://sources",
        "knowledge://source/time",
        "knowledge://collection/time.docs",
        "knowledge://contract",
    ] {
        assert!(knowledge_selection.matches_uri(&veoveo_types::ResourceUri::new(uri).unwrap()));
        for action in [GatewayAction::ResourcesList, GatewayAction::ResourcesRead] {
            assert_eq!(
                decide(&principal, action, &resource("knowledge", uri)),
                PolicyEffect::Allow,
                "{uri}"
            );
        }
    }
    for uri in [
        "knowledge://docs",
        "knowledge://docs/design",
        "knowledge://docs/manual",
    ] {
        assert!(!knowledge_selection.matches_uri(&veoveo_types::ResourceUri::new(uri).unwrap()));
        for action in [GatewayAction::ResourcesList, GatewayAction::ResourcesRead] {
            assert_eq!(
                decide(&principal, action, &resource("knowledge", uri)),
                PolicyEffect::Deny,
                "{uri}"
            );
        }
    }
    for uri in [
        "knowledge://sources{?cursor}",
        "knowledge://source/{server}",
        "knowledge://collection/{collection}",
    ] {
        let target = PolicyTarget::ResourceTemplate {
            server: "knowledge".parse().unwrap(),
            uri: veoveo_types::ResourceTemplateUri::new(uri).unwrap(),
        };
        assert_eq!(
            decide(&principal, GatewayAction::ResourcesTemplatesList, &target),
            PolicyEffect::Allow
        );
        assert_eq!(
            decide(&principal, GatewayAction::CompletionComplete, &target),
            PolicyEffect::Allow
        );
    }
    let indexer = plane
        .oauth_clients
        .iter()
        .find(|c| c.knowledge_indexing.is_some())
        .unwrap();
    let approved = &indexer.knowledge_indexing.as_ref().unwrap().collections;
    let visible = approved
        .iter()
        .filter(|id| {
            veoveo_policy::admit_resource_reads(&catalog, &principal, &profile.id, id.server())
                .is_ok()
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let expected = [
        "time.authority-releases",
        "time.bootstrap-authorities",
        "time.calendars",
        "time.docs",
        "time.epochs",
        "time.events",
    ]
    .into_iter()
    .map(|id| id.parse().unwrap())
    .collect::<BTreeSet<_>>();
    assert_eq!(visible, expected);
    assert!(visible.len() < approved.len());
    for condition in ["role", "tenant", "scope"] {
        let mut denied = principal.clone();
        match condition {
            "role" => denied.roles.clear(),
            "tenant" => denied.tenant = Some("foreign".parse().unwrap()),
            "scope" => {
                denied.scopes.remove(&"knowledge:read".parse().unwrap());
            }
            _ => unreachable!(),
        }
        assert_eq!(
            decide(
                &denied,
                GatewayAction::ResourcesRead,
                &resource("knowledge", "knowledge://sources")
            ),
            PolicyEffect::Deny
        );
    }
}

#[test]
fn isolated_knowledge_pkce_client_preserves_normal_and_machine_authority() {
    use std::collections::BTreeSet;
    use veoveo_gateway_contract::{OAuthClientAuthMethod, OAuthGrantType};
    let plane = plane();
    let catalog = GatewayCatalog::from_control_plane(
        plane.clone(),
        veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
            .bind(veoveo_gateway_catalog::registry().unwrap())
            .unwrap(),
    )
    .unwrap();
    let client = plane
        .oauth_clients
        .iter()
        .find(|c| c.id.as_str() == "knowledge-acceptance-public")
        .unwrap();
    let profile = catalog
        .profile(&"knowledge-acceptance".parse().unwrap())
        .unwrap();
    let normal = plane
        .oauth_clients
        .iter()
        .find(|c| c.id.as_str() == "operator-local-public")
        .unwrap();
    assert_eq!(
        client.allowed_resources,
        BTreeSet::from([profile.protected_resource.clone()])
    );
    assert_eq!(
        client.allowed_scopes,
        ["operator:use", "knowledge:read", "time:read"]
            .into_iter()
            .map(|s| s.parse().unwrap())
            .collect()
    );
    assert_eq!(
        client.grant_types,
        BTreeSet::from([
            OAuthGrantType::AuthorizationCodePkce,
            OAuthGrantType::RefreshToken
        ])
    );
    assert_eq!(
        client.auth_methods,
        BTreeSet::from([OAuthClientAuthMethod::None])
    );
    assert!(client.credential_secret.is_none() && client.knowledge_indexing.is_none());
    assert!(client.jwks.is_none());
    for server in &profile.servers {
        assert!(matches!(server.tools, veoveo_mcp_contract::Exposure::None));
        assert!(matches!(
            server.prompts,
            veoveo_mcp_contract::Exposure::None
        ));
        assert_eq!(server.tasks, veoveo_mcp_contract::TaskExposure::Disabled);
    }
    assert_eq!(client.authorization_server, normal.authorization_server);
    assert_eq!(client.redirect_uris, normal.redirect_uris);
    assert_eq!(client.invocation_mode, veoveo_types::InvocationMode::Direct);
    let principal = ordinary_operator(&plane, client.allowed_scopes.clone());
    let context = catalog.work_context(&client.default_work_context).unwrap();
    assert_eq!(
        context.membership_for(&principal, &client.id),
        Some(WorkContextMembershipLevel::Viewer)
    );
    assert_eq!(
        context.membership_for(&principal, &normal.id),
        Some(WorkContextMembershipLevel::Contributor)
    );
    let ordinary = ordinary_operator(&plane, normal.allowed_scopes.clone());
    let trace = "knowledge-baseline-control".parse().unwrap();
    assert_eq!(
        catalog
            .decide(PolicyRequest {
                principal: &principal,
                profile: &profile.id,
                action: GatewayAction::PromptsList.into(),
                target: &PolicyTarget::Gateway,
                trace_id: &trace,
            })
            .effect,
        PolicyEffect::Allow
    );
    let target = PolicyTarget::Resource {
        server: "knowledge".parse().unwrap(),
        uri: veoveo_types::ResourceUri::new("knowledge://docs/design").unwrap(),
    };
    assert_eq!(
        catalog
            .decide(PolicyRequest {
                principal: &ordinary,
                profile: &"operator".parse().unwrap(),
                action: GatewayAction::ResourcesRead.into(),
                target: &target,
                trace_id: &trace
            })
            .effect,
        PolicyEffect::Allow
    );
    for action in [
        GatewayAction::AdminWrite,
        GatewayAction::TasksCancel,
        GatewayAction::ArtifactUpload,
        GatewayAction::PromptsGet,
    ] {
        assert_eq!(
            catalog
                .decide(PolicyRequest {
                    principal: &principal,
                    profile: &profile.id,
                    action: action.into(),
                    target: &PolicyTarget::Gateway,
                    trace_id: &trace
                })
                .effect,
            PolicyEffect::Deny
        );
    }
    for server in &plane.servers {
        for tool in &server.tools {
            assert_eq!(
                catalog
                    .decide(PolicyRequest {
                        principal: &principal,
                        profile: &profile.id,
                        action: GatewayAction::ToolsCall.into(),
                        target: &PolicyTarget::Tool {
                            server: server.slug.clone(),
                            tool: tool.clone()
                        },
                        trace_id: &trace
                    })
                    .effect,
                PolicyEffect::Deny
            );
        }
    }
    let worker = plane
        .oauth_clients
        .iter()
        .find(|c| c.id.as_str() == "bioma-computers-worker")
        .unwrap();
    assert_eq!(
        worker.allowed_resources,
        ["https://veoveo.bioma.ai/computers/provider"
            .parse()
            .unwrap()]
        .into()
    );
    assert_eq!(
        worker.allowed_scopes,
        ["computers:provider:authenticate".parse().unwrap()].into()
    );
    assert_eq!(
        worker.grant_types,
        [OAuthGrantType::ClientCredentials].into()
    );
    assert_eq!(
        worker.auth_methods,
        [OAuthClientAuthMethod::PrivateKeyJwt].into()
    );
    assert!(
        !normal
            .allowed_resources
            .contains(&profile.protected_resource)
    );
    let indexer = plane
        .oauth_clients
        .iter()
        .find(|c| c.knowledge_indexing.is_some())
        .unwrap();
    assert!(
        !indexer
            .allowed_resources
            .contains(&profile.protected_resource)
    );
}
