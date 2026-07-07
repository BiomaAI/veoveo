use veoveo_mcp_contract::ServerResourceUris;

pub const FRAMES_URI: &str = "coordinates://frames";
pub const CRS_ROOT_URI: &str = "coordinates://crs";
pub const FRAME_TEMPLATE: &str = "coordinates://frame/{frame_id}";
pub const CRS_TEMPLATE: &str = "coordinates://crs/{authority}/{code}";
pub const OPERATION_TEMPLATE: &str = "coordinates://operation/{operation_id}";
pub const ARTIFACT_TEMPLATE: &str = "coordinates://artifact/{sha256}";
pub const USAGE_ROOT_URI: &str = "coordinates://usage";
pub const USAGE_TASK_TEMPLATE: &str = "coordinates://usage/task/{task_id}";

fn coordinates_uris() -> ServerResourceUris {
    ServerResourceUris::new("coordinates")
}

pub fn frame_uri(frame_id: &str) -> String {
    format!("coordinates://frame/{frame_id}")
}

pub fn crs_uri(authority: &str, code: &str) -> String {
    format!("coordinates://crs/{authority}/{code}")
}

pub fn operation_uri(operation_id: &str) -> String {
    format!("coordinates://operation/{operation_id}")
}

pub fn artifact_uri(sha256: &str) -> String {
    coordinates_uris().artifact_uri(sha256)
}

pub fn usage_task_uri(task_id: &str) -> String {
    coordinates_uris().usage_task_uri(task_id)
}

pub fn parse_frame_uri(uri: &str) -> Option<&str> {
    uri.strip_prefix("coordinates://frame/")
        .filter(|frame_id| !frame_id.is_empty() && !frame_id.contains('/'))
}

pub fn parse_crs_uri(uri: &str) -> Option<(&str, &str)> {
    let rest = uri.strip_prefix("coordinates://crs/")?;
    let (authority, code) = rest.split_once('/')?;
    if authority.is_empty() || code.is_empty() || code.contains('/') {
        return None;
    }
    Some((authority, code))
}

pub fn parse_operation_uri(uri: &str) -> Option<&str> {
    uri.strip_prefix("coordinates://operation/")
        .filter(|operation_id| !operation_id.is_empty() && !operation_id.contains('/'))
}

pub fn parse_artifact_uri(uri: &str) -> Option<&str> {
    coordinates_uris().parse_artifact_uri(uri)
}

pub fn parse_usage_task_uri(uri: &str) -> Option<&str> {
    coordinates_uris().parse_usage_task_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_uris_round_trip() {
        assert_eq!(frame_uri("ENU:test"), "coordinates://frame/ENU:test");
        assert_eq!(
            parse_frame_uri("coordinates://frame/ENU:test"),
            Some("ENU:test")
        );
        assert_eq!(
            parse_crs_uri("coordinates://crs/EPSG/4326"),
            Some(("EPSG", "4326"))
        );
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(artifact_uri(sha), format!("coordinates://artifact/{sha}"));
        assert_eq!(parse_artifact_uri(&artifact_uri(sha)), Some(sha));
    }
}
