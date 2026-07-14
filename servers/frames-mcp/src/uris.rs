use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

pub const FRAMES_URI: &str = "frames://frames";
pub const FRAME_TEMPLATE: &str = "frames://frame/{frame_id}";
pub const OPERATION_TEMPLATE: &str = "frames://operation/{operation_id}";
pub const ARTIFACT_TEMPLATE: &str = "frames://artifact/{artifact_id}";
pub const USAGE_ROOT_URI: &str = "frames://usage";
pub const USAGE_TASK_TEMPLATE: &str = "frames://usage/task/{task_id}";

fn frames_uris() -> ServerResourceUris {
    ServerResourceUris::new("frames")
}

pub fn frame_uri(frame_id: &str) -> String {
    format!("frames://frame/{frame_id}")
}

pub fn operation_uri(operation_id: &str) -> String {
    format!("frames://operation/{operation_id}")
}

pub fn artifact_uri(artifact_id: ArtifactId) -> String {
    frames_uris().artifact_uri(artifact_id)
}

pub fn usage_task_uri(task_id: &str) -> String {
    frames_uris().usage_task_uri(task_id)
}

pub fn parse_frame_uri(uri: &str) -> Option<&str> {
    uri.strip_prefix("frames://frame/")
        .filter(|frame_id| !frame_id.is_empty() && !frame_id.contains('/'))
}

pub fn parse_operation_uri(uri: &str) -> Option<&str> {
    uri.strip_prefix("frames://operation/")
        .filter(|operation_id| !operation_id.is_empty() && !operation_id.contains('/'))
}

pub fn parse_artifact_uri(uri: &str) -> Option<ArtifactId> {
    frames_uris().parse_artifact_uri(uri)
}

pub fn parse_usage_task_uri(uri: &str) -> Option<&str> {
    frames_uris().parse_usage_task_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_uris_round_trip() {
        assert_eq!(frame_uri("ENU:test"), "frames://frame/ENU:test");
        assert_eq!(parse_frame_uri("frames://frame/ENU:test"), Some("ENU:test"));
        let artifact_id = ArtifactId::new();
        assert_eq!(
            artifact_uri(artifact_id),
            format!("frames://artifact/{artifact_id}")
        );
        assert_eq!(
            parse_artifact_uri(&artifact_uri(artifact_id)),
            Some(artifact_id)
        );
    }
}
