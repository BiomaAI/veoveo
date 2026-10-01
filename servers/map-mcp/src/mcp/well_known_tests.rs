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
    for id in ["authoring", "acquisition", "routing"] {
        assert!(SERVER_DOCS.doc(id).is_some(), "missing Map document {id}");
    }
    for document in SERVER_DOCS.iter() {
        // Leave space for the observation and the kernel's provenance line.
        assert!(
            document.body.len() + 1024 <= 64 * 1024,
            "Map document {} exceeds the knowledge item budget",
            document.id
        );
    }
}

#[test]
fn contract_declaration_resolves_from_the_embedded_manual() {
    let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
    assert_eq!(declaration.server, "map");
    assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
    for id in ["C17", "C18", "C19", "C20", "C21"] {
        let item = declaration
            .compliance
            .iter()
            .find(|item| item.id == id)
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
