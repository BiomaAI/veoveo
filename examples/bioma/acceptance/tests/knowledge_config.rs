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
