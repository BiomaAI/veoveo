use super::*;
use crate::contract::*;
use std::collections::BTreeMap;
use veoveo_types::ScopeDefinition;

#[test]
fn checked_discovery_is_available_without_store_or_renderer() {
    assert_eq!(SERVER_SETUP.resources().len(), 9);
    assert_eq!(SERVER_SETUP.resource_templates().len(), 7);
    for resource in SERVER_SETUP.resources() {
        assert_eq!(
            resource.address().to_uri().unwrap().as_str(),
            resource.descriptor().uri
        );
    }
    let capability = SERVER_SETUP
        .server_config()
        .capabilities
        .resources
        .as_ref()
        .unwrap();
    assert_eq!(capability.subscribe, Some(true));
    assert_ne!(capability.list_changed, Some(true));
    assert_eq!(
        SERVER_SETUP.scope_names(),
        &ViewScope::ALL
            .iter()
            .map(|scope| scope.name().clone())
            .collect()
    );
    let baseline = BTreeSet::from([ScopeName::new("operator:use").unwrap()]);
    assert_eq!(visible_resources(&baseline).len(), 8);
    let granted = BTreeSet::from([ViewScope::Capture.name().clone()]);
    let resources = visible_resources(&granted);
    assert_eq!(resources.len(), 9);
    let app = resources
        .iter()
        .find(|item| item.uri == uris::PREVIEW_APP_URI)
        .unwrap();
    assert_eq!(app.title.as_deref(), Some("Preview"));
    assert!(app.icons.as_ref().is_some_and(|icons| !icons.is_empty()));
}

#[test]
fn all_advertised_templates_expand_to_the_typed_builder_spelling() {
    let composition = SceneCompositionId::from_stable_key(b"setup");
    let tile = TileKey::from_bytes(b"tile");
    let variables = BTreeMap::from([
        ("doc_id".into(), "agents".into()),
        ("layer_id".into(), "base".into()),
        ("composition_id".into(), composition.to_string()),
        ("view_id".into(), "view-1".into()),
        ("frame_id".into(), "frame-1".into()),
        ("tile_key".into(), tile.to_string()),
        ("width_px".into(), "1280".into()),
        ("height_px".into(), "720".into()),
        ("max_screen_error_px".into(), "16".into()),
    ]);
    for template in SERVER_SETUP.resource_templates() {
        let uri = template.template().expand_scalars(&variables).unwrap();
        assert_eq!(ViewResource::parse(&uri).unwrap().to_uri().unwrap(), uri);
    }
}

#[test]
fn gateway_registrations_agree_with_the_checked_discovery_contract() {
    for source in [
        include_str!("../../../../../configs/gateway.local.json"),
        include_str!("../../../../../examples/bioma/gateway.json"),
    ] {
        let configuration: veoveo_mcp_contract::GatewayControlPlane =
            serde_json::from_str(source).unwrap();
        configuration.validate().unwrap();
        let server = configuration
            .servers
            .iter()
            .find(|server| server.slug.as_str() == "view")
            .unwrap();
        assert!(!server.capabilities.resources_list_changed);
        let metadata = serde_json::to_value(&server.metadata).unwrap();
        assert_eq!(
            metadata["contract_revision"],
            veoveo_mcp_contract::docs::CONTRACT_REVISION
        );
    }
}
