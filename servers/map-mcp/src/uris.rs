pub const DATASETS_URI: &str = "map://datasets";
pub const SOURCES_URI: &str = "map://sources";
pub const ACQUISITIONS_URI: &str = "map://acquisitions";
pub const ACTIVE_RELEASES_URI: &str = "map://active-releases";
pub const LOCATIONS_URI: &str = "map://locations";
pub const FACILITIES_URI: &str = "map://facilities";
pub const MOBILITY_PROFILES_URI: &str = "map://mobility-profiles";
pub const RESTRICTIONS_URI: &str = "map://restrictions";
pub const ROUTES_URI: &str = "map://routes";
pub const MATRICES_URI: &str = "map://matrices";
pub const FEATURE_LAYERS_URI: &str = "map://feature-layers";
pub const PUBLICATIONS_URI: &str = "map://publications";
pub const LAYER_PRODUCTS_URI: &str = "map://layer-products";
pub const COMPOSITIONS_URI: &str = "map://compositions";

pub const SOURCE_TEMPLATE: &str = "map://source/{source_id}";
pub const ACQUISITION_TEMPLATE: &str = "map://acquisition/{acquisition_id}";
pub const DATASET_TEMPLATE: &str = "map://dataset/{dataset_id}";
pub const RELEASE_TEMPLATE: &str = "map://dataset/{dataset_id}/release/{release_id}";
pub const LOCATION_TEMPLATE: &str = "map://location/{location_id}";
pub const FACILITY_TEMPLATE: &str = "map://facility/{facility_id}";
pub const MOBILITY_PROFILE_TEMPLATE: &str = "map://mobility-profile/{profile_id}/{profile_version}";
pub const RESTRICTION_TEMPLATE: &str = "map://restriction/{restriction_id}";
pub const ROUTE_TEMPLATE: &str = "map://route/{route_id}";
pub const MATRIX_TEMPLATE: &str = "map://matrix/{matrix_id}";
pub const ARTIFACT_TEMPLATE: &str = "map://artifact/{artifact_id}";
pub const FEATURE_LAYER_TEMPLATE: &str = "map://feature-layer/{layer_id}";
pub const FEATURE_SCHEMA_TEMPLATE: &str = "map://feature-layer/{layer_id}/schema/{schema_version}";
pub const FEATURE_STYLE_TEMPLATE: &str = "map://feature-layer/{layer_id}/style/{style_version}";
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

/// The map administration MCP App view. The slug segment matches the
/// gateway's ServerOwned `ui://{slug}/{page}` projection.
pub const ADMIN_APP_URI: &str = "ui://map/admin.html";

pub fn source_uri(id: &str) -> String {
    format!("map://source/{id}")
}

pub fn acquisition_uri(id: &str) -> String {
    format!("map://acquisition/{id}")
}

pub fn dataset_uri(id: &str) -> String {
    format!("map://dataset/{id}")
}

pub fn release_uri(dataset_id: &str, release_id: &str) -> String {
    format!("map://dataset/{dataset_id}/release/{release_id}")
}

pub fn location_uri(id: &str) -> String {
    format!("map://location/{id}")
}

pub fn facility_uri(id: &str) -> String {
    format!("map://facility/{id}")
}

pub fn mobility_profile_uri(id: &str, version: u64) -> String {
    format!("map://mobility-profile/{id}/{version}")
}

pub fn restriction_uri(id: &str) -> String {
    format!("map://restriction/{id}")
}

pub fn route_uri(id: &str) -> String {
    format!("map://route/{id}")
}

pub fn matrix_uri(id: &str) -> String {
    format!("map://matrix/{id}")
}

pub fn feature_layer_uri(id: &str) -> String {
    format!("map://feature-layer/{id}")
}

pub fn feature_schema_uri(layer_id: &str, version: u64) -> String {
    format!("map://feature-layer/{layer_id}/schema/{version}")
}

pub fn feature_style_uri(layer_id: &str, version: u64) -> String {
    format!("map://feature-layer/{layer_id}/style/{version}")
}

pub fn features_uri(layer_id: &str) -> String {
    format!("map://feature-layer/{layer_id}/features")
}

pub fn feature_uri(layer_id: &str, feature_id: &str) -> String {
    format!("map://feature-layer/{layer_id}/feature/{feature_id}")
}

pub fn feature_revision_uri(layer_id: &str, feature_id: &str, revision: u64) -> String {
    format!("map://feature-layer/{layer_id}/feature/{feature_id}/revision/{revision}")
}

pub fn changeset_uri(layer_id: &str, changeset_id: &str) -> String {
    format!("map://feature-layer/{layer_id}/changeset/{changeset_id}")
}

pub fn publication_uri(layer_id: &str, publication_id: &str) -> String {
    format!("map://feature-layer/{layer_id}/publication/{publication_id}")
}

pub fn layer_product_uri(layer_id: &str, publication_id: &str, product_id: &str) -> String {
    format!("map://feature-layer/{layer_id}/publication/{publication_id}/product/{product_id}")
}

pub fn composition_uri(composition_id: &str) -> String {
    format!("map://composition/{composition_id}")
}

pub fn composition_revision_uri(composition_id: &str, revision: u64) -> String {
    format!("map://composition/{composition_id}/revision/{revision}")
}

pub fn parse_artifact(uri: &str) -> Option<veoveo_mcp_contract::ArtifactId> {
    veoveo_mcp_contract::ServerResourceUris::new("map").parse_artifact_uri(uri)
}

pub fn parse_single<'a>(uri: &'a str, prefix: &str) -> Option<&'a str> {
    let value = uri.strip_prefix(prefix)?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

pub fn parse_release(uri: &str) -> Option<(&str, &str)> {
    let suffix = uri.strip_prefix("map://dataset/")?;
    let (dataset, release) = suffix.split_once("/release/")?;
    (!dataset.is_empty() && !release.is_empty() && !dataset.contains('/') && !release.contains('/'))
        .then_some((dataset, release))
}

pub fn parse_profile(uri: &str) -> Option<(&str, u64)> {
    let suffix = uri.strip_prefix("map://mobility-profile/")?;
    let (id, version) = suffix.split_once('/')?;
    Some((id, version.parse().ok()?))
}

pub fn parse_feature_layer(uri: &str) -> Option<&str> {
    parse_single(uri, "map://feature-layer/")
}

pub fn parse_feature_schema(uri: &str) -> Option<(&str, u64)> {
    parse_layer_version(uri, "/schema/")
}

pub fn parse_feature_style(uri: &str) -> Option<(&str, u64)> {
    parse_layer_version(uri, "/style/")
}

pub fn parse_features(uri: &str) -> Option<&str> {
    let suffix = uri.strip_prefix("map://feature-layer/")?;
    let layer_id = suffix.strip_suffix("/features")?;
    valid_segment(layer_id).then_some(layer_id)
}

pub fn parse_features_request(
    uri: &str,
) -> Result<Option<crate::contract::QueryFeaturesRequest>, String> {
    let parsed =
        url::Url::parse(uri).map_err(|error| format!("invalid feature query URI: {error}"))?;
    if parsed.scheme() != "map" || parsed.host_str() != Some("feature-layer") {
        return Ok(None);
    }
    let segments = parsed
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    if segments.len() != 2 || segments[1] != "features" {
        return Ok(None);
    }
    if parsed.fragment().is_some() {
        return Err("feature query URI must not contain a fragment".to_owned());
    }
    let layer_id = segments[0]
        .parse()
        .map_err(|error| format!("invalid layer_id: {error}"))?;
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
    let mut seen = std::collections::BTreeSet::new();
    for (name, value) in parsed.query_pairs() {
        if !seen.insert(name.to_string()) {
            return Err(format!(
                "feature query parameter `{name}` appears more than once"
            ));
        }
        match name.as_ref() {
            "publication_id" => {
                request.publication_id = Some(
                    value
                        .parse()
                        .map_err(|error| format!("invalid publication_id: {error}"))?,
                );
            }
            "bbox" => request.bbox = Some(parse_bbox(&value)?),
            "datetime" => request.datetime = Some(parse_datetime_interval(&value)?),
            "geometry_type" => {
                request.geometry_type = Some(
                    serde_json::from_value(serde_json::Value::String(value.into_owned()))
                        .map_err(|error| format!("invalid geometry_type: {error}"))?,
                );
            }
            "filter" => {
                request.filter = Some(
                    serde_json::from_str(&value)
                        .map_err(|error| format!("invalid CQL2 JSON filter: {error}"))?,
                );
            }
            "limit" => {
                request.limit = value
                    .parse()
                    .map_err(|_| "feature query limit must be an integer".to_owned())?;
            }
            "cursor" => request.cursor = Some(value.into_owned()),
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

pub fn parse_feature(uri: &str) -> Option<(&str, &str)> {
    let suffix = uri.strip_prefix("map://feature-layer/")?;
    let (layer_id, feature_id) = suffix.split_once("/feature/")?;
    (valid_segment(layer_id) && valid_segment(feature_id)).then_some((layer_id, feature_id))
}

pub fn parse_feature_revision(uri: &str) -> Option<(&str, &str, u64)> {
    let suffix = uri.strip_prefix("map://feature-layer/")?;
    let (layer_id, remainder) = suffix.split_once("/feature/")?;
    let (feature_id, revision) = remainder.split_once("/revision/")?;
    if !valid_segment(layer_id) || !valid_segment(feature_id) {
        return None;
    }
    Some((layer_id, feature_id, revision.parse().ok()?))
}

pub fn parse_changeset(uri: &str) -> Option<(&str, &str)> {
    parse_layer_identity(uri, "/changeset/")
}

pub fn parse_publication(uri: &str) -> Option<(&str, &str)> {
    parse_layer_identity(uri, "/publication/")
}

pub fn parse_layer_product(uri: &str) -> Option<(&str, &str, &str)> {
    let suffix = uri.strip_prefix("map://feature-layer/")?;
    let (layer_id, remainder) = suffix.split_once("/publication/")?;
    let (publication_id, product_id) = remainder.split_once("/product/")?;
    (valid_segment(layer_id) && valid_segment(publication_id) && valid_segment(product_id))
        .then_some((layer_id, publication_id, product_id))
}

pub fn parse_composition(uri: &str) -> Option<&str> {
    parse_single(uri, "map://composition/")
}

pub fn parse_composition_revision(uri: &str) -> Option<(&str, u64)> {
    let suffix = uri.strip_prefix("map://composition/")?;
    let (composition_id, revision) = suffix.split_once("/revision/")?;
    if !valid_segment(composition_id) {
        return None;
    }
    Some((composition_id, revision.parse().ok()?))
}

fn parse_layer_version<'a>(uri: &'a str, separator: &str) -> Option<(&'a str, u64)> {
    let suffix = uri.strip_prefix("map://feature-layer/")?;
    let (layer_id, version) = suffix.split_once(separator)?;
    if !valid_segment(layer_id) {
        return None;
    }
    Some((layer_id, version.parse().ok()?))
}

fn parse_layer_identity<'a>(uri: &'a str, separator: &str) -> Option<(&'a str, &'a str)> {
    let suffix = uri.strip_prefix("map://feature-layer/")?;
    let (layer_id, identity) = suffix.split_once(separator)?;
    (valid_segment(layer_id) && valid_segment(identity)).then_some((layer_id, identity))
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty() && !value.contains('/') && !value.contains('?') && !value.contains('#')
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers_reject_extra_segments() {
        assert_eq!(
            parse_single("map://route/route-1", "map://route/"),
            Some("route-1")
        );
        assert!(parse_single("map://route/route-1/x", "map://route/").is_none());
        assert_eq!(
            parse_release("map://dataset/dataset-1/release/release-1"),
            Some(("dataset-1", "release-1"))
        );
    }

    #[test]
    fn feature_query_uri_parses_standard_filters() {
        let request = parse_features_request(
            "map://feature-layer/feature-layer-019be7be-68f8-7000-8000-000000000001/features?bbox=170,-10,-170,10&datetime=../2026-07-22T00%3A00%3A00Z&limit=25",
        )
        .unwrap()
        .unwrap();
        assert_eq!(request.limit, 25);
        assert_eq!(request.bbox.unwrap().west, 170.0);
        assert!(
            request.datetime.unwrap().interval[0]
                .as_timestamp()
                .is_none()
        );
    }
}
