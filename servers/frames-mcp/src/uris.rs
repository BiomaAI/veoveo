use veoveo_mcp_contract::{
    ArtifactId, FrameWorldRevisionUri, FrameWorldUri, ServerResourceUris, WorldFrameUri,
};

pub const WORLDS_URI: &str = "frames://worlds";
pub const WORLD_TEMPLATE: &str = "frames://world/{world_id}";
pub const WORLD_REVISION_TEMPLATE: &str = "frames://world/{world_id}/revision/{revision_id}";
pub const WORLD_FRAME_TEMPLATE: &str =
    "frames://world/{world_id}/revision/{revision_id}/frame/{frame_id}";
pub const OPERATION_TEMPLATE: &str = "frames://operation/{operation_id}";
pub const ARTIFACT_TEMPLATE: &str = "frames://artifact/{artifact_id}";
pub const USAGE_ROOT_URI: &str = "frames://usage";
pub const USAGE_TASK_TEMPLATE: &str = "frames://usage/task/{task_id}";

fn frames_uris() -> ServerResourceUris {
    ServerResourceUris::new("frames")
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

pub fn parse_world_uri(uri: &str) -> Option<FrameWorldUri> {
    FrameWorldUri::parse(uri.to_owned()).ok()
}

pub fn parse_world_revision_uri(uri: &str) -> Option<FrameWorldRevisionUri> {
    FrameWorldRevisionUri::parse(uri.to_owned()).ok()
}

pub fn parse_world_frame_uri(uri: &str) -> Option<WorldFrameUri> {
    WorldFrameUri::parse(uri.to_owned()).ok()
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
    use veoveo_mcp_contract::{FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri};

    #[test]
    fn world_uris_round_trip() {
        let world_id = FrameWorldId::new("uav-showcase-new-york").unwrap();
        let world_uri = FrameWorldUri::new(&world_id);
        assert_eq!(parse_world_uri(world_uri.as_str()), Some(world_uri));

        let revision_uri = FrameWorldRevisionUri::new(
            &world_id,
            &FrameWorldRevisionId::new("revision-1").unwrap(),
        );
        assert_eq!(
            parse_world_revision_uri(revision_uri.as_str()),
            Some(revision_uri.clone())
        );
        let frame_uri = WorldFrameUri::new(&revision_uri, &FrameId::new("isaac-world").unwrap());
        assert_eq!(parse_world_frame_uri(frame_uri.as_str()), Some(frame_uri));

        let artifact_id = ArtifactId::new();
        assert_eq!(
            parse_artifact_uri(&artifact_uri(artifact_id)),
            Some(artifact_id)
        );
    }
}
