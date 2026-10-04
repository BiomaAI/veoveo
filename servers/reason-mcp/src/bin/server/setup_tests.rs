use super::*;
use std::collections::BTreeSet;
use veoveo_types::{ResourceAddress, ScopeName};

#[test]
fn checked_setup_owns_the_declared_resources_templates_and_capabilities() {
    let setup = &*SERVER_SETUP;
    assert_eq!(setup.resources().len(), 10);
    assert_eq!(setup.resource_templates().len(), 11);
    let addresses = setup
        .resources()
        .iter()
        .map(|item| item.descriptor().uri.as_str())
        .collect::<BTreeSet<_>>();
    assert!(addresses.contains(uris::DOCS_URI));
    assert!(addresses.contains(uris::CONTRACT_URI));
    assert!(addresses.contains(uris::ANALYSES_APP_URI));
    for resource in setup.resources() {
        assert_eq!(
            resource.address().to_uri().unwrap().as_str(),
            resource.descriptor().uri
        );
    }
    let capabilities = &setup.server_config().capabilities;
    assert_eq!(
        capabilities.resources.as_ref().unwrap().subscribe,
        Some(true)
    );
    assert_ne!(
        capabilities.resources.as_ref().unwrap().list_changed,
        Some(true)
    );
    assert_eq!(
        setup.documents().contract_declaration().contract_revision,
        veoveo_mcp_contract::docs::CONTRACT_REVISION
    );
    assert!(ReasonScope::try_from(&ScopeName::new("operator:use").unwrap()).is_err());
}

#[test]
fn static_discovery_changes_and_unknown_resource_families_are_not_admitted() {
    use crate::resources::accepted_subscription_filter;
    use rmcp::model::SubscriptionFilter;
    use veoveo_reason_mcp::contract::AnalysisId;

    assert!(
        accepted_subscription_filter(
            &SubscriptionFilter::builder()
                .resources_list_changed()
                .build()
        )
        .is_none()
    );
    let id = AnalysisId::parse("01983da0-0000-7000-8000-000000000001").unwrap();
    let uri = uris::analysis_uri(id).to_string();
    let requested = SubscriptionFilter::builder()
        .resources_list_changed()
        .resource_subscriptions([
            uri.clone(),
            uris::MODELS_URI.into(),
            "reason://analysis/task-1".into(),
        ])
        .task_ids(["opaque-mcp-task".to_owned()])
        .build();
    let accepted = accepted_subscription_filter(&requested).unwrap();
    assert_eq!(accepted.resource_subscriptions, Some(vec![uri]));
    assert_eq!(accepted.task_ids, requested.task_ids);
    assert_ne!(accepted.resources_list_changed, Some(true));
}

#[test]
fn gateway_registrations_match_the_checked_discovery_profile() {
    #[derive(serde::Deserialize)]
    struct Metadata {
        contract_revision: u64,
    }
    for source in [
        include_str!("../../../../../configs/gateway.local.json"),
        include_str!("../../../../../examples/bioma/gateway.json"),
    ] {
        let configuration: veoveo_mcp_contract::GatewayControlPlane =
            serde_json::from_str(source).unwrap();
        configuration
            .validate(&veoveo_gateway_catalog::registry().unwrap())
            .unwrap();
        let server = configuration
            .servers
            .iter()
            .find(|server| server.slug.as_str() == "reason")
            .unwrap();
        assert!(!server.capabilities.resources_list_changed);
        let metadata: Metadata =
            serde_json::from_value(serde_json::to_value(&server.metadata).unwrap()).unwrap();
        assert_eq!(
            metadata.contract_revision,
            u64::from(veoveo_mcp_contract::docs::CONTRACT_REVISION)
        );
    }
}

#[test]
fn finding_collections_declare_typed_roots_members_and_subscription_admission() {
    use veoveo_reason_mcp::contract::{
        AnalysisId, FindingCollection, FindingCursor, FindingResource,
    };
    let id: AnalysisId = "01983da0-0000-7000-8000-000000000001".parse().unwrap();
    let time = "2026-10-01T00:00:00Z".parse().unwrap();
    for collection in FindingCollection::ALL {
        let expected = collection.descriptor();
        let descriptor = SERVER_SETUP
            .resource_templates()
            .iter()
            .find(|t| t.descriptor().uri_template.as_str() == collection.member_template())
            .unwrap()
            .descriptor();
        assert_eq!(
            veoveo_mcp_knowledge_extension::client::collection(descriptor).unwrap(),
            Some(expected)
        );
        for address in [
            FindingResource::root(collection),
            FindingResource::Member {
                collection,
                analysis: id,
            },
        ] {
            assert_eq!(
                crate::subscriptions::finding(address.to_uri().unwrap().as_str()),
                Some(address)
            );
        }
        let page = FindingResource::Page {
            cursor: FindingCursor::new(collection, time, id),
        };
        assert!(crate::subscriptions::finding(page.to_uri().unwrap().as_str()).is_none());
    }
}
