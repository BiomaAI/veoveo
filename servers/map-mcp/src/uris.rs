pub const DATASETS_URI: &str = "map://datasets";
pub const SOURCES_URI: &str = "map://sources";
pub const LOCATIONS_URI: &str = "map://locations";
pub const FACILITIES_URI: &str = "map://facilities";
pub const MOBILITY_PROFILES_URI: &str = "map://mobility-profiles";
pub const RESTRICTIONS_URI: &str = "map://restrictions";
pub const ROUTES_URI: &str = "map://routes";
pub const MATRICES_URI: &str = "map://matrices";

pub const SOURCE_TEMPLATE: &str = "map://source/{source_id}";
pub const DATASET_TEMPLATE: &str = "map://dataset/{dataset_id}";
pub const RELEASE_TEMPLATE: &str = "map://dataset/{dataset_id}/release/{release_id}";
pub const LOCATION_TEMPLATE: &str = "map://location/{location_id}";
pub const FACILITY_TEMPLATE: &str = "map://facility/{facility_id}";
pub const MOBILITY_PROFILE_TEMPLATE: &str = "map://mobility-profile/{profile_id}/{profile_version}";
pub const RESTRICTION_TEMPLATE: &str = "map://restriction/{restriction_id}";
pub const ROUTE_TEMPLATE: &str = "map://route/{route_id}";
pub const MATRIX_TEMPLATE: &str = "map://matrix/{matrix_id}";
pub const ARTIFACT_TEMPLATE: &str = "map://artifact/{artifact_id}";

pub fn source_uri(id: &str) -> String {
    format!("map://source/{id}")
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
}
