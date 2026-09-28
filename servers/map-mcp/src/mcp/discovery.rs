//! Static resource discovery and templates, selected by current domain grants.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct ResourceDiscoveryAccess {
    pub(super) admin: bool,
    pub(super) dataset_read: bool,
    pub(super) feature_read: bool,
    pub(super) spatial_derive: bool,
}

impl ResourceDiscoveryAccess {
    pub(super) fn from_identity(identity: &GatewayInternalIdentity) -> Self {
        Self {
            admin: identity_has_scope(identity, MapScope::Admin),
            dataset_read: identity_has_scope(identity, MapScope::DatasetRead),
            feature_read: identity_has_scope(identity, MapScope::FeatureRead),
            spatial_derive: identity_has_scope(identity, MapScope::SpatialDerive),
        }
    }
}

/// Protocol discovery remains constant in tenant data size. Instance
/// resources stay directly addressable through templates and bounded root
/// indexes; listing the MCP surface never scans the Map catalog or DuckDB.
pub(super) fn discoverable_resources(
    access: ResourceDiscoveryAccess,
    basemap: &crate::contract::MapWorkspaceBasemap,
) -> Vec<Resource> {
    let mut resources = well_known_resources();
    if access.admin || access.dataset_read || access.feature_read {
        let basemap_origin = basemap.origin().expect("validated workspace basemap");
        resources.push(
            veoveo_mcp_apps_extension::app_resource_with_meta(
                uris::WORKSPACE_APP_URI,
                "map-workspace-app",
                veoveo_mcp_apps_extension::ResourceUiMeta {
                    csp: Some(veoveo_mcp_apps_extension::UiCsp {
                        connect_domains: vec![basemap_origin.clone()],
                        resource_domains: vec![basemap_origin],
                        ..Default::default()
                    }),
                    prefers_border: None,
                },
            )
            .with_title("Map Explorer")
            .with_description(
                "Explore maps and manage geographic layers, saved views, data sources, and releases.",
            )
            .with_icons(vec![rmcp::model::Icon::new(WORKSPACE_APP_ICON)]),
        );
        resources.push(json_resource_descriptor(
            uris::WORKSPACE_URI.to_owned(),
            "Map workspace access".to_owned(),
            "Caller-specific Map workspace capabilities.",
        ));
    }
    if access.admin {
        resources.push(json_resource_descriptor(
            uris::ACQUISITIONS_URI.to_owned(),
            "Map acquisitions".to_owned(),
            "Governed acquisition jobs (map:admin).",
        ));
    }
    if access.dataset_read {
        resources.extend(root_resources());
        resources.push(json_resource_descriptor(
            uris::ACTIVE_RELEASES_URI.to_owned(),
            "Active releases".to_owned(),
            "Active immutable dataset release pointers.",
        ));
        resources.push(json_resource_descriptor(
            uris::RASTER_DERIVATIONS_URI.to_owned(),
            "Raster derivations".to_owned(),
            "Work Context-scoped governed raster derivations.",
        ));
        if access.spatial_derive {
            resources.push(json_resource_descriptor(
                uris::SPATIAL_DERIVATIONS_URI.to_owned(),
                "Spatial derivations".to_owned(),
                "Work Context-scoped advisory geometry and mobility validation.",
            ));
        }
    }
    if access.feature_read {
        resources.push(json_resource_descriptor(
            uris::FEATURE_LAYERS_URI.to_owned(),
            "Authored feature layers".to_owned(),
            "Work Context-scoped mutable layer heads and immutable revision links.",
        ));
        resources.push(json_resource_descriptor(
            uris::PUBLICATIONS_URI.to_owned(),
            "Feature layer publications".to_owned(),
            "Immutable published layer revisions.",
        ));
        resources.push(json_resource_descriptor(
            uris::LAYER_PRODUCTS_URI.to_owned(),
            "Feature layer products".to_owned(),
            "Immutable artifacts derived from published feature layers.",
        ));
        resources.push(json_resource_descriptor(
            uris::COMPOSITIONS_URI.to_owned(),
            "Map compositions".to_owned(),
            "Work Context-scoped maps built from immutable publication pins.",
        ));
    }
    resources.sort_by(|left, right| left.uri.cmp(&right.uri));
    resources
}

/// Every advertised resource template. `list_resource_templates` serves this
/// list and the `map://contract` capability inventory declares it, so the two
/// cannot diverge.
pub(super) fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(
            crate::contract::MapTravelModelsUri::TEMPLATE,
            "Travel-model page",
        )
        .with_description("A page of 100 caller-owned completed travel models.")
        .with_mime_type("application/json"),
        ResourceTemplate::new(uris::DOC_TEMPLATE, "Server document")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        template(
            uris::SOURCE_TEMPLATE,
            "Map source",
            "Authorized source provenance.",
        ),
        template(
            uris::ACQUISITION_TEMPLATE,
            "Map acquisition",
            "Governed acquisition job (map:admin).",
        ),
        template(
            uris::DATASETS_PAGE_TEMPLATE,
            "Dataset release page",
            "100 tenant-visible releases per page, ordered by release ID.",
        ),
        template(
            uris::DATASET_TEMPLATE,
            "Map dataset",
            "100 releases per page for one dataset, ordered by release ID.",
        ),
        template(
            uris::RELEASE_TEMPLATE,
            "Dataset release",
            "Immutable governed release.",
        ),
        template(
            uris::SOURCE_FEATURE_TEMPLATE,
            "Immutable source feature",
            "Complete normalized source feature pinned to one dataset release.",
        ),
        template(
            uris::RASTER_TEMPLATE,
            "Immutable raster product",
            "Release-pinned raster metadata, provenance, and artifact identity.",
        ),
        template(
            uris::RASTER_DERIVATIONS_PAGE_TEMPLATE,
            "Raster derivation page",
            "100 Work Context-owned derivation summaries per page.",
        ),
        template(
            uris::SPATIAL_DERIVATIONS_PAGE_TEMPLATE,
            "Spatial derivation page",
            "100 Work Context-owned derivation summaries per page.",
        ),
        template(
            uris::RASTER_DERIVATION_TEMPLATE,
            "Governed raster derivation",
            "Immutable raster operation, source, parameters, algorithm, and output artifact.",
        ),
        template(
            uris::SPATIAL_DERIVATION_TEMPLATE,
            "Governed spatial derivation",
            "Immutable advisory geometry, mobility findings, source pins, and algorithm identity.",
        ),
        template(
            uris::LOCATION_TEMPLATE,
            "Map location",
            "Named location with lineage.",
        ),
        template(
            uris::FACILITY_TEMPLATE,
            "Map facility",
            "Logistics facility.",
        ),
        template(
            uris::MOBILITY_PROFILE_TEMPLATE,
            "Mobility profile",
            "Versioned mobility constraints.",
        ),
        template(
            uris::RESTRICTION_TEMPLATE,
            "Map restriction",
            "Effective restriction.",
        ),
        template(
            uris::ROUTES_PAGE_TEMPLATE,
            "Route page",
            "100 owner-visible route summaries per page.",
        ),
        template(
            uris::MATRICES_PAGE_TEMPLATE,
            "Matrix page",
            "100 owner-visible matrix summaries per page.",
        ),
        template(
            uris::ACQUISITIONS_PAGE_TEMPLATE,
            "Acquisition page",
            "100 owner-visible acquisition jobs per page (map:admin).",
        ),
        template(uris::ROUTE_TEMPLATE, "Map route", "Owner-scoped route."),
        template(
            uris::MATRIX_TEMPLATE,
            "Route matrix",
            "Owner-scoped route matrix.",
        ),
        template(
            uris::TRAVEL_MODEL_TEMPLATE,
            "Optimization travel model",
            "Immutable cuOpt-ready cost and transit-time matrix manifest.",
        ),
        template(
            uris::ARTIFACT_TEMPLATE,
            "Map artifact",
            "Governed immutable map artifact.",
        ),
        template(
            uris::FEATURE_LAYERS_PAGE_TEMPLATE,
            "Feature layer page",
            "100 visible, unarchived layer documents per page.",
        ),
        template(
            uris::PUBLICATIONS_PAGE_TEMPLATE,
            "Publication page",
            "100 visible publications per page, optionally for one layer.",
        ),
        template(
            uris::LAYER_PRODUCTS_PAGE_TEMPLATE,
            "Layer product page",
            "100 visible products per page, optionally for one publication.",
        ),
        template(
            uris::COMPOSITIONS_PAGE_TEMPLATE,
            "Composition page",
            "100 visible, unarchived composition documents per page.",
        ),
        template(
            uris::FEATURE_LAYER_TEMPLATE,
            "Authored feature layer",
            "Work Context-scoped layer head with pinned schema and style revisions.",
        ),
        template(
            uris::FEATURE_SCHEMA_TEMPLATE,
            "Feature schema revision",
            "Immutable JSON Schema 2020-12 property contract.",
        ),
        template(
            uris::FEATURE_STYLE_TEMPLATE,
            "Feature style revision",
            "Immutable safe map style revision.",
        ),
        template(
            uris::FEATURE_STYLE_REVISION_TEMPLATE,
            "Feature style revision by identity",
            "Immutable safe map style revision referenced by a publication or composition.",
        ),
        template(
            uris::FEATURES_TEMPLATE,
            "Authored feature query",
            "Paginated current or published GeoJSON features with spatial, temporal, and CQL2 filters.",
        ),
        template(
            uris::FEATURE_TEMPLATE,
            "Authored feature",
            "Current canonical feature head.",
        ),
        template(
            uris::FEATURE_REVISION_TEMPLATE,
            "Authored feature revision",
            "Immutable canonical feature revision.",
        ),
        template(
            uris::CHANGESET_TEMPLATE,
            "Feature changeset",
            "Atomic authored feature commit.",
        ),
        template(
            uris::PUBLICATION_TEMPLATE,
            "Feature layer publication",
            "Immutable published layer revision.",
        ),
        template(
            uris::LAYER_PRODUCT_TEMPLATE,
            "Feature layer product",
            "Immutable artifact derived from a published layer revision.",
        ),
        template(
            uris::COMPOSITION_TEMPLATE,
            "Map composition",
            "Mutable head of a governed publication-pinned map composition.",
        ),
        template(
            uris::COMPOSITION_REVISION_TEMPLATE,
            "Map composition revision",
            "Immutable map composition revision.",
        ),
    ]
}

fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![json_resource_descriptor(
        uris::DOCS_URI.to_owned(),
        "Server documents".to_owned(),
        "Index of the crate documents embedded at build time.",
    )];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(uris::doc_uri(doc.id), doc.title)
                .with_title(doc.title)
                .with_description("Crate document embedded at build time.")
                .with_mime_type("text/markdown"),
        );
    }
    resources.push(json_resource_descriptor(
        uris::CONTRACT_URI.to_owned(),
        "Contract declaration".to_owned(),
        "Machine-readable contract revision, compliance, and capability inventory.",
    ));
    resources
}

#[cfg(test)]
pub(super) fn stable_resource_uris() -> Vec<String> {
    let mut resource_uris: Vec<String> = well_known_resources()
        .into_iter()
        .map(|resource| resource.uri.clone())
        .collect();
    resource_uris.extend(
        [
            uris::WORKSPACE_APP_URI,
            uris::WORKSPACE_URI,
            uris::ACQUISITIONS_URI,
            uris::ACTIVE_RELEASES_URI,
            uris::FEATURE_LAYERS_URI,
            uris::PUBLICATIONS_URI,
            uris::LAYER_PRODUCTS_URI,
            uris::COMPOSITIONS_URI,
            uris::RASTER_DERIVATIONS_URI,
            uris::SPATIAL_DERIVATIONS_URI,
        ]
        .map(str::to_owned),
    );
    resource_uris.extend(
        root_resources()
            .into_iter()
            .map(|resource| resource.uri.clone()),
    );
    resource_uris.sort();
    resource_uris
}

fn root_resources() -> Vec<Resource> {
    [
        (uris::DATASETS_URI, "Map datasets"),
        (uris::SOURCES_URI, "Map sources"),
        (uris::LOCATIONS_URI, "Map locations"),
        (uris::FACILITIES_URI, "Map facilities"),
        (uris::MOBILITY_PROFILES_URI, "Mobility profiles"),
        (uris::RESTRICTIONS_URI, "Map restrictions"),
        (uris::ROUTES_URI, "Map routes"),
        (uris::MATRICES_URI, "Route matrices"),
        (uris::TRAVEL_MODELS_URI, "Optimization travel models"),
        (uris::RASTERS_URI, "Raster products"),
    ]
    .into_iter()
    .map(|(uri, title)| {
        json_resource_descriptor(
            uri.to_owned(),
            title.to_owned(),
            "Authorized Map domain index.",
        )
    })
    .collect()
}

fn json_resource_descriptor(uri: String, title: String, description: &str) -> Resource {
    Resource::new(uri, title.clone())
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}

fn template(uri: &str, title: &str, description: &str) -> ResourceTemplate {
    ResourceTemplate::new(uri, title)
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}
