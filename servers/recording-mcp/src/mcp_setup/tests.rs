use super::*;
use crate::contract::{RecordingCatalogCursor, RecordingId};
use std::collections::{BTreeMap, BTreeSet};
use veoveo_types::{ResourceAddress, ScopeDefinition, ScopeName};

#[test]
fn checked_discovery_needs_no_store_cache_or_redap_instance() {
    assert_eq!(SERVER_SETUP.resources().len(), 6);
    assert_eq!(SERVER_SETUP.resource_templates().len(), 4);
    for resource in SERVER_SETUP.resources() {
        assert_eq!(
            resource.address().to_uri().unwrap().as_str(),
            resource.descriptor().uri
        );
    }
    let capabilities = SERVER_SETUP
        .server_config()
        .capabilities
        .resources
        .as_ref()
        .unwrap();
    assert_eq!(capabilities.subscribe, Some(true));
    assert_ne!(capabilities.list_changed, Some(true));
    assert_eq!(
        SERVER_SETUP.scope_names(),
        &BTreeSet::from([RecordingScope::Seal.name().clone()])
    );
    let mut grants = BTreeSet::from([ScopeName::new("admin:manage").unwrap()]);
    assert!(!SERVER_SETUP.has_scope(&grants, RecordingScope::Seal));
    grants.insert(RecordingScope::Seal.into());
    assert!(SERVER_SETUP.has_scope(&grants, RecordingScope::Seal));
}

#[test]
fn every_advertised_template_expands_to_an_owner_address() {
    let id = RecordingId::new();
    let cursor = RecordingCatalogCursor::new("2026-09-28T12:00:00Z".parse().unwrap(), id);
    let variables = BTreeMap::from([
        ("doc_id".into(), "agents".into()),
        ("recording_id".into(), id.to_string()),
        ("cursor".into(), cursor.to_string()),
    ]);
    for template in SERVER_SETUP.resource_templates() {
        let uri = template.template().expand_scalars(&variables).unwrap();
        assert_eq!(
            RecordingResource::parse(&uri).unwrap().to_uri().unwrap(),
            uri
        );
    }
}

#[test]
fn registrations_and_admin_clients_require_the_recording_seal_permission() {
    for source in [
        include_str!("../../../../configs/gateway.local.json"),
        include_str!("../../../../examples/bioma/gateway.json"),
        include_str!("../../../../testing/fixtures/catalog-installation/gateway.json"),
    ] {
        let configuration: veoveo_mcp_contract::GatewayControlPlane =
            serde_json::from_str(source).unwrap();
        configuration.validate().unwrap();
        let server = configuration
            .servers
            .iter()
            .find(|server| server.slug.as_str() == "recording")
            .unwrap();
        assert!(!server.capabilities.resources_list_changed);
        assert_eq!(
            serde_json::to_value(&server.metadata).unwrap()["contract_revision"],
            veoveo_mcp_contract::docs::CONTRACT_REVISION
        );

        let mut sealing_rules = 0;
        for policy in &configuration.policies {
            for rule in &policy.rules {
                if rule
                    .tools
                    .iter()
                    .any(|tool| tool.as_str() == "seal_recording")
                {
                    sealing_rules += 1;
                    for scope in [
                        "operator:use",
                        "admin:manage",
                        RecordingScope::Seal.name().as_str(),
                    ] {
                        assert!(
                            rule.required_scopes
                                .contains(&ScopeName::new(scope).unwrap()),
                            "{}: {scope}",
                            rule.id
                        );
                    }
                    assert_eq!(
                        rule.profiles,
                        BTreeSet::from([
                            veoveo_mcp_contract::GatewayProfileId::new("admin").unwrap()
                        ])
                    );
                }
            }
        }
        let has_admin = configuration
            .profiles
            .iter()
            .any(|profile| profile.id.as_str() == "admin");
        assert_eq!(sealing_rules, if has_admin { 2 } else { 0 });
        for client in &configuration.oauth_clients {
            assert_eq!(
                client.allowed_scopes.contains(RecordingScope::Seal.name()),
                client
                    .allowed_scopes
                    .contains(&ScopeName::new("admin:manage").unwrap()),
                "{}",
                client.id
            );
        }
    }
}
