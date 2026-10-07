use veoveo_mcp_contract::docs::{
    CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
};

use super::{ResourceDiscoveryAccess, SERVER_DOCS, discoverable_resources, stable_resource_uris};
use crate::{contract::MapWorkspaceBasemap, uris};

#[test]
fn derivation_collections_accept_the_notifications_the_workers_emit() {
    assert!(super::is_subscribable(uris::RASTER_DERIVATIONS_URI));
    assert!(super::is_subscribable(uris::SPATIAL_DERIVATIONS_URI));
    assert!(!super::is_subscribable("map://spatial-derivations/extra"));
    assert!(!super::is_subscribable(uris::DOCS_URI));
}

#[test]
fn knowledge_templates_declare_bounded_summaries_and_subscribable_pages() {
    use crate::contract::{MapKnowledgeCollection, MapKnowledgePageUri};
    use veoveo_mcp_knowledge_extension::{ChangeSignal, CollectionDescriptor, EXTENSION_ID};
    let templates = super::resource_templates();
    for collection in MapKnowledgeCollection::ALL {
        let member = templates
            .iter()
            .find(|template| template.uri_template == collection.member_template())
            .unwrap();
        let declaration: CollectionDescriptor =
            serde_json::from_value(member.meta.as_ref().unwrap()[EXTENSION_ID].clone()).unwrap();
        assert_eq!(declaration, collection.descriptor());
        assert!(
            templates
                .iter()
                .any(|template| template.uri_template == collection.page_template())
        );
        if declaration.change_signal() == ChangeSignal::Listen {
            assert!(super::is_subscribable(
                MapKnowledgePageUri::new(collection).to_uri().as_str()
            ));
        }
    }
}

#[test]
fn subscription_addresses_apply_the_resource_owners_admission() {
    let route = crate::contract::MapRouteUri::new(crate::contract::RouteId::new());
    let restriction =
        crate::contract::MapRestrictionUri::new(crate::contract::RestrictionId::new());
    for address in [route.as_str(), restriction.as_str()] {
        assert!(super::is_subscribable(address));
        assert!(!super::is_subscribable(&format!("{address}?secret=value")));
        assert!(!super::is_subscribable(&format!("{address}#fragment")));
    }
    assert!(!super::is_subscribable("map://route/arbitrary"));
    assert!(!super::is_subscribable("map://restriction/arbitrary"));
}

#[test]
fn embedded_documents_carry_the_crate_manual_and_design() {
    assert_eq!(SERVER_DOCS.server(), "map");
    let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
    assert!(agents.body.contains("## Contract Compliance"));
    let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
    assert!(!design.body.is_empty());
    let index = SERVER_DOCS.llms_txt();
    assert!(index.contains("(agents)"));
    assert!(index.contains("(design)"));
    for id in ["authoring", "acquisition", "routing", "resources"] {
        assert!(SERVER_DOCS.doc(id).is_some(), "missing Map document {id}");
    }
    for document in SERVER_DOCS.iter() {
        use rmcp::model::{
            ClientCapabilities, ReadResourceRequestParams, ReadResourceResponse, RequestMetaObject,
            ResourceContents,
        };
        use veoveo_mcp_knowledge_extension::{self as knowledge, DocumentId};

        let scheme = veoveo_types::ResourceScheme::parse("map").unwrap();
        let uri = knowledge::docs::member_uri(&scheme, &DocumentId::parse(document.id).unwrap());
        let mut meta = RequestMetaObject::default();
        let mut capabilities = ClientCapabilities::default();
        capabilities
            .extensions
            .get_or_insert_default()
            .insert(knowledge::EXTENSION_ID.into(), Default::default());
        meta.set_client_capabilities(capabilities);
        let response = SERVER_DOCS
            .read_authorized_knowledge(
                &scheme,
                &ReadResourceRequestParams::new(uri.as_str()),
                &meta,
            )
            .unwrap()
            .unwrap();
        let ReadResourceResponse::Complete(result) = response else {
            panic!("document read must complete");
        };
        let observation = knowledge::client::validate_read(&result, &uri, None)
            .unwrap()
            .expect("negotiated document observation");
        assert_eq!(observation.content_sha256(), &document.digest);
        let [ResourceContents::TextResourceContents { text, .. }] = result.contents.as_slice()
        else {
            panic!("document read must return one text item");
        };
        assert_eq!(text, document.body);
        // Knowledge caps source text; Kernel accounts for model-read provenance separately.
        assert!(
            text.len() <= 64 * 1024,
            "Map document {} exceeds the knowledge source text budget",
            document.id
        );
    }
}

#[test]
fn contract_declaration_resolves_from_the_embedded_manual() {
    let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
    assert_eq!(declaration.server().as_str(), "map");
    assert_eq!(declaration.contract_revision(), CONTRACT_REVISION);
    for id in ["C17", "C18", "C19", "C20", "C21"] {
        let item = declaration
            .compliance()
            .iter()
            .find(|item| item.id.as_str() == id)
            .expect("declared checklist item");
        assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
    }
    let json = serde_json::to_value(&declaration).expect("declaration serializes");
    assert_eq!(json["server"], "map");
}

#[test]
fn contract_declaration_defers_runtime_surface_to_discover() {
    let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
    let json = serde_json::to_value(declaration).unwrap();
    assert!(json.get("capabilities").is_none());
}

#[test]
fn resource_discovery_is_bounded_by_the_protocol_surface() {
    let basemap = MapWorkspaceBasemap::open_free_map(
        "https://tiles.openfreemap.org/styles/positron",
        "https://tiles.openfreemap.org/styles/dark",
    )
    .unwrap();
    let resources = discoverable_resources(
        ResourceDiscoveryAccess {
            admin: true,
            dataset_read: true,
            feature_read: true,
            spatial_derive: true,
        },
        &basemap,
    );
    assert_eq!(resources.len(), stable_resource_uris().len());
    assert!(resources.len() < 32);
    assert!(resources.windows(2).all(|pair| pair[0].uri < pair[1].uri));
    assert!(
        resources
            .iter()
            .any(|resource| resource.uri == uris::DATASETS_URI)
    );
    assert!(
        resources
            .iter()
            .all(|resource| !resource.uri.starts_with("map://release/"))
    );
}

#[test]
fn checked_setup_preserves_each_discovery_grant_and_configured_app_origin() {
    use super::setup::SERVER_SETUP;
    use crate::contract::{MapAddress, MapScope};
    use veoveo_types::{ResourceTemplateUri, ScopeDefinition};
    assert_eq!(SERVER_SETUP.scope_names().len(), MapScope::ALL.len());
    for scope in MapScope::ALL {
        assert!(SERVER_SETUP.has_scope(
            &std::collections::BTreeSet::from([scope.name().clone()]),
            *scope
        ));
        assert!(!SERVER_SETUP.has_scope(&std::collections::BTreeSet::new(), *scope));
    }
    let basemap = MapWorkspaceBasemap::open_free_map(
        "https://maps.example.test/light",
        "https://maps.example.test/dark",
    )
    .unwrap();
    for mask in 0..16 {
        let access = ResourceDiscoveryAccess {
            admin: mask & 1 != 0,
            dataset_read: mask & 2 != 0,
            feature_read: mask & 4 != 0,
            spatial_derive: mask & 8 != 0,
        };
        let actual = discoverable_resources(access, &basemap);
        let visible = actual
            .iter()
            .map(|r| r.uri.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for uri in [uris::DOCS_URI, uris::CONTRACT_URI] {
            assert!(visible.contains(uri));
        }
        for (uri, allowed) in [
            (uris::WORKSPACE_APP_URI, mask & 7 != 0),
            (uris::WORKSPACE_URI, mask & 7 != 0),
            (uris::ACQUISITIONS_URI, mask & 1 != 0),
            (uris::SOURCES_URI, mask & 2 != 0),
            (uris::ACTIVE_RELEASES_URI, mask & 2 != 0),
            (uris::FEATURE_LAYERS_URI, mask & 4 != 0),
            (uris::PUBLICATIONS_URI, mask & 4 != 0),
            (uris::LAYER_PRODUCTS_URI, mask & 4 != 0),
            (uris::COMPOSITIONS_URI, mask & 4 != 0),
            (uris::SPATIAL_DERIVATIONS_URI, mask & 10 == 10),
        ] {
            assert_eq!(visible.contains(uri), allowed, "mask {mask}, {uri}");
        }
        for resource in actual {
            MapAddress::parse(&resource.uri).unwrap();
            if resource.uri == uris::WORKSPACE_APP_URI {
                let meta = serde_json::to_string(&resource.meta).unwrap();
                assert!(meta.contains("https://maps.example.test"));
            }
        }
    }
    for resource in SERVER_SETUP.resources() {
        assert_eq!(
            resource.address().to_uri().as_str(),
            resource.descriptor().uri
        );
    }
    for template in SERVER_SETUP.resource_templates() {
        ResourceTemplateUri::new(&template.descriptor().uri_template).unwrap();
    }
}
