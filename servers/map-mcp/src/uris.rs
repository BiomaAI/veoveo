use crate::contract::{
    AcquisitionId, FacilityId, FeatureChangeSetId, FeatureLayerId, LayerProductId,
    LayerPublicationId, LocationId, MapCompositionId, MapDatasetId, MapDocument, MapFeatureId,
    MapResource, RouteMatrixId, StyleRevisionId,
};
/// Well-known surface roots (contract C18, C19). These literals must match
/// `veoveo_mcp_contract::ServerResourceUris::new(SCHEME.clone())`. Checked setup
/// validates these descriptors before startup.
pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::new("map").expect("declared server resource scheme")
    });

pub const DOCS_URI: &str = "map://docs";
pub const CONTRACT_URI: &str = "map://contract";

pub const DATASETS_URI: &str = "map://datasets";
pub const SOURCES_URI: &str = crate::contract::MapSourcesUri::ROOT;
pub const ACQUISITIONS_URI: &str = "map://acquisitions";
pub const ACTIVE_RELEASES_URI: &str = "map://active-releases";
pub const LOCATIONS_URI: &str = "map://locations";
pub const FACILITIES_URI: &str = "map://facilities";
pub const MOBILITY_PROFILES_URI: &str = crate::contract::MapMobilityProfilesUri::ROOT;
pub const RESTRICTIONS_URI: &str = crate::contract::MapRestrictionsUri::ROOT;
pub const ROUTES_URI: &str = "map://routes";
pub const MATRICES_URI: &str = "map://matrices";
pub const TRAVEL_MODELS_URI: &str = "map://travel-models";
pub const RASTERS_URI: &str = "map://rasters";
pub const RASTER_DERIVATIONS_URI: &str = "map://raster-derivations";
pub const SPATIAL_DERIVATIONS_URI: &str = "map://spatial-derivations";
pub const FEATURE_LAYERS_URI: &str = "map://feature-layers";
pub const PUBLICATIONS_URI: &str = "map://publications";
pub const LAYER_PRODUCTS_URI: &str = "map://layer-products";
pub const COMPOSITIONS_URI: &str = "map://compositions";
pub const WORKSPACE_URI: &str = "map://workspace";

pub const DOC_TEMPLATE: &str = "map://docs/{doc_id}";
pub const SOURCE_TEMPLATE: &str = crate::contract::MapSourceUri::TEMPLATE;
pub const ACQUISITION_TEMPLATE: &str = "map://acquisition/{acquisition_id}";
pub const DATASETS_PAGE_TEMPLATE: &str = "map://datasets{?cursor}";
pub const DATASET_TEMPLATE: &str = "map://dataset/{dataset_id}{?cursor}";
pub const RELEASE_TEMPLATE: &str = crate::contract::MapReleaseUri::TEMPLATE;
pub const SOURCE_FEATURE_TEMPLATE: &str = crate::contract::MapSourceFeatureUri::TEMPLATE;
pub const RASTER_TEMPLATE: &str = crate::contract::MapRasterUri::TEMPLATE;
pub const RASTER_DERIVATIONS_PAGE_TEMPLATE: &str = "map://raster-derivations{?cursor}";
pub const SPATIAL_DERIVATIONS_PAGE_TEMPLATE: &str = "map://spatial-derivations{?cursor}";
pub const RASTER_DERIVATION_TEMPLATE: &str = crate::contract::MapRasterDerivationUri::TEMPLATE;
pub const SPATIAL_DERIVATION_TEMPLATE: &str = crate::contract::MapSpatialDerivationUri::TEMPLATE;
pub const LOCATION_TEMPLATE: &str = "map://location/{location_id}";
pub const FACILITY_TEMPLATE: &str = "map://facility/{facility_id}";
pub const MOBILITY_PROFILE_TEMPLATE: &str = crate::contract::MapMobilityProfileUri::TEMPLATE;
pub const RESTRICTION_TEMPLATE: &str = crate::contract::MapRestrictionUri::TEMPLATE;
pub const ROUTES_PAGE_TEMPLATE: &str = "map://routes{?cursor}";
pub const MATRICES_PAGE_TEMPLATE: &str = "map://matrices{?cursor}";
pub const ACQUISITIONS_PAGE_TEMPLATE: &str = "map://acquisitions{?cursor}";
pub const ROUTE_TEMPLATE: &str = crate::contract::MapRouteUri::TEMPLATE;
pub const MATRIX_TEMPLATE: &str = "map://matrix/{matrix_id}";
pub const TRAVEL_MODEL_TEMPLATE: &str = "map://travel-model/{travel_model_id}";
pub const ARTIFACT_TEMPLATE: &str = "map://artifact/{artifact_id}";
pub const FEATURE_LAYERS_PAGE_TEMPLATE: &str = "map://feature-layers{?cursor}";
pub const PUBLICATIONS_PAGE_TEMPLATE: &str = "map://publications{?layer_id,cursor}";
pub const LAYER_PRODUCTS_PAGE_TEMPLATE: &str = "map://layer-products{?publication_id,cursor}";
pub const COMPOSITIONS_PAGE_TEMPLATE: &str = "map://compositions{?cursor}";
pub const FEATURE_LAYER_TEMPLATE: &str = "map://feature-layer/{layer_id}";
pub const FEATURE_SCHEMA_TEMPLATE: &str = "map://feature-layer/{layer_id}/schema/{schema_version}";
pub const FEATURE_STYLE_TEMPLATE: &str = "map://feature-layer/{layer_id}/style/{style_version}";
pub const FEATURE_STYLE_REVISION_TEMPLATE: &str = "map://feature-style/{style_revision_id}";
pub const FEATURES_TEMPLATE: &str = "map://feature-layer/{layer_id}/features{?publication_id,bbox,datetime,geometry_type,filter,limit,cursor,minimum_commit_sequence}";
pub const FEATURE_TEMPLATE: &str = "map://feature-layer/{layer_id}/feature/{feature_id}";
pub const FEATURE_REVISION_TEMPLATE: &str =
    "map://feature-layer/{layer_id}/feature/{feature_id}/revision/{feature_revision}";
pub const CHANGESET_TEMPLATE: &str = "map://feature-layer/{layer_id}/changeset/{changeset_id}";
pub const PUBLICATION_TEMPLATE: &str =
    "map://feature-layer/{layer_id}/publication/{publication_id}";
pub const LAYER_PRODUCT_TEMPLATE: &str =
    "map://feature-layer/{layer_id}/publication/{publication_id}/product/{product_id}";
pub const COMPOSITION_TEMPLATE: &str = "map://composition/{composition_id}";
pub const COMPOSITION_REVISION_TEMPLATE: &str =
    "map://composition/{composition_id}/revision/{composition_revision}";

/// The single Map MCP App. The slug segment matches the gateway's
/// ServerOwned `ui://{slug}/{page}` projection.
pub const WORKSPACE_APP_URI: &str = "ui://map/workspace.html";

pub fn doc_uri(document: MapDocument) -> String {
    MapResource::Document(document).to_uri().to_string()
}

pub fn acquisition_uri(id: &AcquisitionId) -> String {
    MapResource::Acquisition { id: id.clone() }
        .to_uri()
        .to_string()
}

pub fn dataset_uri(id: &MapDatasetId) -> String {
    MapResource::Dataset { id: id.clone() }.to_uri().to_string()
}

pub fn location_uri(id: &LocationId) -> String {
    MapResource::Location { id: id.clone() }
        .to_uri()
        .to_string()
}

pub fn facility_uri(id: &FacilityId) -> String {
    MapResource::Facility { id: id.clone() }
        .to_uri()
        .to_string()
}

pub fn matrix_uri(id: &RouteMatrixId) -> String {
    MapResource::Matrix { id: id.clone() }.to_uri().to_string()
}

pub fn feature_layer_uri(layer: &FeatureLayerId) -> String {
    MapResource::Layer {
        layer: layer.clone(),
    }
    .to_uri()
    .to_string()
}

pub fn feature_schema_uri(layer: &FeatureLayerId, version: u64) -> String {
    MapResource::Schema {
        layer: layer.clone(),
        version,
    }
    .to_uri()
    .to_string()
}

pub fn feature_style_uri(layer: &FeatureLayerId, version: u64) -> String {
    MapResource::Style {
        layer: layer.clone(),
        version,
    }
    .to_uri()
    .to_string()
}

pub fn feature_style_revision_uri(id: &StyleRevisionId) -> String {
    MapResource::StyleRevision { id: id.clone() }
        .to_uri()
        .to_string()
}

pub fn features_uri(layer: &FeatureLayerId) -> String {
    MapResource::Features {
        layer: layer.clone(),
    }
    .to_uri()
    .to_string()
}

pub fn feature_uri(layer: &FeatureLayerId, feature: &MapFeatureId) -> String {
    MapResource::Feature {
        layer: layer.clone(),
        feature: feature.clone(),
    }
    .to_uri()
    .to_string()
}

pub fn feature_revision_uri(
    layer: &FeatureLayerId,
    feature: &MapFeatureId,
    revision: u64,
) -> String {
    MapResource::FeatureRevision {
        layer: layer.clone(),
        feature: feature.clone(),
        revision,
    }
    .to_uri()
    .to_string()
}

pub fn changeset_uri(layer: &FeatureLayerId, changeset: &FeatureChangeSetId) -> String {
    MapResource::Changeset {
        layer: layer.clone(),
        changeset: changeset.clone(),
    }
    .to_uri()
    .to_string()
}

pub fn publication_uri(layer: &FeatureLayerId, publication: &LayerPublicationId) -> String {
    MapResource::Publication {
        layer: layer.clone(),
        publication: publication.clone(),
    }
    .to_uri()
    .to_string()
}

pub fn layer_product_uri(
    layer: &FeatureLayerId,
    publication: &LayerPublicationId,
    product: &LayerProductId,
) -> String {
    MapResource::Product {
        layer: layer.clone(),
        publication: publication.clone(),
        product: product.clone(),
    }
    .to_uri()
    .to_string()
}

pub fn composition_uri(id: &MapCompositionId) -> String {
    MapResource::Composition { id: id.clone() }
        .to_uri()
        .to_string()
}

pub fn composition_revision_uri(id: &MapCompositionId, revision: u64) -> String {
    MapResource::CompositionRevision {
        id: id.clone(),
        revision,
    }
    .to_uri()
    .to_string()
}

pub fn parse_artifact(uri: &str) -> Option<veoveo_artifact_contract::ArtifactId> {
    let address = veoveo_artifact_contract::ArtifactUri::parse(uri).ok()?;
    match address.address() {
        veoveo_artifact_contract::ArtifactAddress::Presented {
            scheme,
            artifact_id,
        } if scheme == &*SCHEME => Some(*artifact_id),
        _ => None,
    }
}

pub fn parse_doc(uri: &str) -> Option<MapDocument> {
    match MapResource::parse(uri).ok()? {
        MapResource::Document(document) => Some(document),
        _ => None,
    }
}

pub fn parse_acquisition(uri: &str) -> Option<AcquisitionId> {
    match MapResource::parse(uri).ok()? {
        MapResource::Acquisition { id } => Some(id),
        _ => None,
    }
}

pub fn parse_dataset(uri: &str) -> Option<MapDatasetId> {
    match MapResource::parse(uri).ok()? {
        MapResource::Dataset { id } => Some(id),
        _ => None,
    }
}

pub fn parse_location(uri: &str) -> Option<LocationId> {
    match MapResource::parse(uri).ok()? {
        MapResource::Location { id } => Some(id),
        _ => None,
    }
}

pub fn parse_facility(uri: &str) -> Option<FacilityId> {
    match MapResource::parse(uri).ok()? {
        MapResource::Facility { id } => Some(id),
        _ => None,
    }
}

pub fn parse_matrix(uri: &str) -> Option<RouteMatrixId> {
    match MapResource::parse(uri).ok()? {
        MapResource::Matrix { id } => Some(id),
        _ => None,
    }
}

pub fn parse_feature_layer(uri: &str) -> Option<FeatureLayerId> {
    match MapResource::parse(uri).ok()? {
        MapResource::Layer { layer } => Some(layer),
        _ => None,
    }
}

pub fn parse_feature_schema(uri: &str) -> Option<(FeatureLayerId, u64)> {
    match MapResource::parse(uri).ok()? {
        MapResource::Schema { layer, version } => Some((layer, version)),
        _ => None,
    }
}

pub fn parse_feature_style(uri: &str) -> Option<(FeatureLayerId, u64)> {
    match MapResource::parse(uri).ok()? {
        MapResource::Style { layer, version } => Some((layer, version)),
        _ => None,
    }
}

pub fn parse_feature_style_revision(uri: &str) -> Option<StyleRevisionId> {
    match MapResource::parse(uri).ok()? {
        MapResource::StyleRevision { id } => Some(id),
        _ => None,
    }
}

pub fn parse_features(uri: &str) -> Option<FeatureLayerId> {
    match MapResource::parse(uri).ok()? {
        MapResource::Features { layer } => Some(layer),
        _ => None,
    }
}

pub fn parse_feature(uri: &str) -> Option<(FeatureLayerId, MapFeatureId)> {
    match MapResource::parse(uri).ok()? {
        MapResource::Feature { layer, feature } => Some((layer, feature)),
        _ => None,
    }
}

pub fn parse_feature_revision(uri: &str) -> Option<(FeatureLayerId, MapFeatureId, u64)> {
    match MapResource::parse(uri).ok()? {
        MapResource::FeatureRevision {
            layer,
            feature,
            revision,
        } => Some((layer, feature, revision)),
        _ => None,
    }
}

pub fn parse_changeset(uri: &str) -> Option<(FeatureLayerId, FeatureChangeSetId)> {
    match MapResource::parse(uri).ok()? {
        MapResource::Changeset { layer, changeset } => Some((layer, changeset)),
        _ => None,
    }
}

pub fn parse_publication(uri: &str) -> Option<(FeatureLayerId, LayerPublicationId)> {
    match MapResource::parse(uri).ok()? {
        MapResource::Publication { layer, publication } => Some((layer, publication)),
        _ => None,
    }
}

pub fn parse_layer_product(
    uri: &str,
) -> Option<(FeatureLayerId, LayerPublicationId, LayerProductId)> {
    match MapResource::parse(uri).ok()? {
        MapResource::Product {
            layer,
            publication,
            product,
        } => Some((layer, publication, product)),
        _ => None,
    }
}

pub fn parse_composition(uri: &str) -> Option<MapCompositionId> {
    match MapResource::parse(uri).ok()? {
        MapResource::Composition { id } => Some(id),
        _ => None,
    }
}

pub fn parse_composition_revision(uri: &str) -> Option<(MapCompositionId, u64)> {
    match MapResource::parse(uri).ok()? {
        MapResource::CompositionRevision { id, revision } => Some((id, revision)),
        _ => None,
    }
}

pub fn parse_features_request(
    uri: &str,
) -> Result<Option<crate::contract::QueryFeaturesRequest>, String> {
    let parsed = veoveo_types::ResourceUriParts::parse(uri)
        .map_err(|_| "invalid feature query URI".to_owned())?;
    if parsed.scheme() != "map" || parsed.authority() != "feature-layer" {
        return Ok(None);
    }
    let segments = parsed.path_segments().collect::<Vec<_>>();
    if segments.len() != 2 || segments[1] != "features" {
        return Ok(None);
    }
    let layer_id: FeatureLayerId = segments[0]
        .parse()
        .map_err(|_| "invalid feature query layer identity".to_owned())?;
    if (parsed.has_query() && parsed.query_parameters().is_empty())
        || segments
            .iter()
            .any(|segment| matches!(segment, std::borrow::Cow::Owned(_)))
    {
        return Err("invalid feature query URI".to_owned());
    }
    let mut request = crate::contract::QueryFeaturesRequest {
        layer_id,
        publication_id: None,
        bbox: None,
        datetime: None,
        geometry_type: None,
        filter: None,
        limit: 100,
        cursor: None,
        minimum_commit_sequence: None,
    };
    for (name, value) in parsed.query_parameters() {
        match name.as_str() {
            "publication_id" => {
                request.publication_id = Some(
                    value
                        .parse()
                        .map_err(|error| format!("invalid publication_id: {error}"))?,
                );
            }
            "bbox" => request.bbox = Some(parse_bbox(value)?),
            "datetime" => request.datetime = Some(parse_datetime_interval(value)?),
            "geometry_type" => {
                request.geometry_type = Some(
                    serde_json::from_value(serde_json::Value::String(value.to_owned()))
                        .map_err(|error| format!("invalid geometry_type: {error}"))?,
                );
            }
            "filter" => {
                request.filter = Some(
                    serde_json::from_str(value)
                        .map_err(|error| format!("invalid CQL2 JSON filter: {error}"))?,
                );
            }
            "limit" => {
                request.limit = value
                    .parse()
                    .map_err(|_| "feature query limit must be an integer".to_owned())?;
            }
            "cursor" => request.cursor = Some(value.to_owned()),
            "minimum_commit_sequence" => {
                request.minimum_commit_sequence = Some(value.parse().map_err(|_| {
                    "minimum_commit_sequence must be a non-negative integer".to_owned()
                })?);
            }
            other => return Err(format!("unsupported feature query parameter `{other}`")),
        }
    }
    Ok(Some(request))
}

fn parse_bbox(value: &str) -> Result<crate::contract::Wgs84BoundingBox, String> {
    let values = value
        .split(',')
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| "bbox must contain four finite numbers".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let [west, south, east, north] = values.as_slice() else {
        return Err("bbox must contain west,south,east,north".to_owned());
    };
    let bbox = crate::contract::Wgs84BoundingBox {
        west: *west,
        south: *south,
        east: *east,
        north: *north,
    };
    bbox.validate().map_err(|error| error.to_string())?;
    Ok(bbox)
}

fn parse_datetime_interval(value: &str) -> Result<crate::contract::FeatureTime, String> {
    let (start, end) = value
        .split_once('/')
        .ok_or_else(|| "datetime must use start/end interval form".to_owned())?;
    let parse_bound = |bound: &str| {
        if bound == ".." || bound.is_empty() {
            Ok(crate::contract::JsonFgTimeBoundary::open())
        } else {
            chrono::DateTime::parse_from_rfc3339(bound)
                .map(|value| {
                    crate::contract::JsonFgTimeBoundary::timestamp(
                        value.with_timezone(&chrono::Utc),
                    )
                })
                .map_err(|error| format!("invalid datetime bound: {error}"))
        }
    };
    let interval = crate::contract::FeatureTime {
        interval: [parse_bound(start)?, parse_bound(end)?],
    };
    interval.validate().map_err(|error| error.to_string())?;
    Ok(interval)
}
