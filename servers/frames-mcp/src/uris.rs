use veoveo_mcp_contract::{
    ArtifactId, FrameWorldRevisionUri, FrameWorldUri, ServerResourceUris, WorldFrameUri,
};

pub const WORLDS_URI: &str = "frames://worlds";
pub const WORKSPACE_APP_URI: &str = "ui://frames/workspace.html";
pub const WORLD_TEMPLATE: &str = "frames://world/{world_id}";
pub const WORLD_REVISION_TEMPLATE: &str = "frames://world/{world_id}/revision/{revision_id}";
pub const WORLD_FRAME_TEMPLATE: &str =
    "frames://world/{world_id}/revision/{revision_id}/frame/{frame_id}";
pub const OPERATION_TEMPLATE: &str = "frames://operation/{operation_id}";
pub const ARTIFACT_TEMPLATE: &str = "frames://artifact/{artifact_id}";
pub const USAGE_ROOT_URI: &str = "frames://usage";
pub const USAGE_TASK_TEMPLATE: &str = "frames://usage/task/{task_id}";

/// Well-known surface roots (contract C18, C19). These literals must match
/// `ServerResourceUris::new("frames")`; a unit test below pins the
/// equivalence.
pub const DOCS_URI: &str = "frames://docs";
pub const CONTRACT_URI: &str = "frames://contract";
pub const DOC_TEMPLATE: &str = "frames://docs/{doc_id}";

fn frames_uris() -> ServerResourceUris {
    ServerResourceUris::new("frames")
}

pub fn operation_uri(operation_id: &str) -> String {
    format!("frames://operation/{operation_id}")
}

pub fn doc_uri(doc_id: &str) -> String {
    frames_uris().doc_uri(doc_id)
}

pub fn parse_doc_uri(uri: &str) -> Option<&str> {
    veoveo_mcp_contract::parse_server_doc_uri("frames", uri)
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
    fn well_known_uris_match_the_shared_contract_conventions() {
        let conventions = frames_uris();
        assert_eq!(DOCS_URI, conventions.docs_root_uri());
        assert_eq!(CONTRACT_URI, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(doc_uri("agents"), conventions.doc_uri("agents"));
        assert_eq!(parse_doc_uri("frames://docs/agents"), Some("agents"));
        assert_eq!(parse_doc_uri("frames://docs"), None);
        assert_eq!(parse_doc_uri("frames://docs/agents/extra"), None);
    }

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
