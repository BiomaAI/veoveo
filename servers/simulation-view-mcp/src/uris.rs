use veoveo_mcp_contract::{LiveCameraId, LiveSessionId, LiveViewId};

pub const SESSIONS: &str = "simulation-view://sessions";
pub const CAPACITY: &str = "simulation-view://capacity";
pub const DOCS: &str = "simulation-view://docs";
pub const CONTRACT: &str = "simulation-view://contract";
pub const LIVE_APP_URI: &str = "ui://simulation-view/live.html";

pub const SESSION_TEMPLATE: &str = "simulation-view://session/{session_id}";
pub const SCENE_TEMPLATE: &str = "simulation-view://session/{session_id}/scene";
pub const POSE_SOURCE_TEMPLATE: &str = "simulation-view://session/{session_id}/pose-source";
pub const CAMERAS_TEMPLATE: &str = "simulation-view://session/{session_id}/cameras";
pub const CAMERA_TEMPLATE: &str = "simulation-view://session/{session_id}/camera/{camera_id}";
pub const STREAMS_TEMPLATE: &str = "simulation-view://session/{session_id}/streams";
pub const STREAM_TEMPLATE: &str = "simulation-view://session/{session_id}/stream/{stream_id}";
pub const DOC_TEMPLATE: &str = "simulation-view://docs/{doc_id}";

const SESSION_PREFIX: &str = "simulation-view://session/";

pub fn session(session_id: &LiveSessionId) -> String {
    format!("{SESSION_PREFIX}{session_id}")
}

pub fn scene(session_id: &LiveSessionId) -> String {
    format!("{}/{suffix}", session(session_id), suffix = "scene")
}

pub fn pose_source(session_id: &LiveSessionId) -> String {
    format!("{}/{suffix}", session(session_id), suffix = "pose-source")
}

pub fn cameras(session_id: &LiveSessionId) -> String {
    format!("{}/{suffix}", session(session_id), suffix = "cameras")
}

pub fn camera(session_id: &LiveSessionId, camera_id: &LiveCameraId) -> String {
    format!("{}/camera/{camera_id}", session(session_id))
}

pub fn streams(session_id: &LiveSessionId) -> String {
    format!("{}/{suffix}", session(session_id), suffix = "streams")
}

pub fn stream(session_id: &LiveSessionId, stream_id: &LiveViewId) -> String {
    format!("{}/stream/{stream_id}", session(session_id))
}

pub fn parse_session(uri: &str) -> Option<LiveSessionId> {
    parse_tail(uri, SESSION_PREFIX).and_then(|value| value.parse().ok())
}

pub fn parse_scene(uri: &str) -> Option<LiveSessionId> {
    parse_session_suffix(uri, "scene")
}

pub fn parse_pose_source(uri: &str) -> Option<LiveSessionId> {
    parse_session_suffix(uri, "pose-source")
}

pub fn parse_cameras(uri: &str) -> Option<LiveSessionId> {
    parse_session_suffix(uri, "cameras")
}

pub fn parse_camera(uri: &str) -> Option<(LiveSessionId, LiveCameraId)> {
    let (session_id, camera_id) = parse_pair(uri, "camera")?;
    Some((session_id.parse().ok()?, camera_id.parse().ok()?))
}

pub fn parse_streams(uri: &str) -> Option<LiveSessionId> {
    parse_session_suffix(uri, "streams")
}

pub fn parse_stream(uri: &str) -> Option<(LiveSessionId, LiveViewId)> {
    let (session_id, stream_id) = parse_pair(uri, "stream")?;
    Some((session_id.parse().ok()?, stream_id.parse().ok()?))
}

pub fn parse_doc(uri: &str) -> Option<&str> {
    veoveo_mcp_contract::parse_server_doc_uri("simulation-view", uri)
}

fn parse_tail<'a>(uri: &'a str, prefix: &str) -> Option<&'a str> {
    let value = uri.strip_prefix(prefix)?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

fn parse_session_suffix(uri: &str, suffix: &str) -> Option<LiveSessionId> {
    let value = uri.strip_prefix(SESSION_PREFIX)?;
    let session_id = value.strip_suffix(&format!("/{suffix}"))?;
    (!session_id.is_empty() && !session_id.contains('/'))
        .then(|| session_id.parse().ok())
        .flatten()
}

fn parse_pair<'a>(uri: &'a str, kind: &str) -> Option<(&'a str, &'a str)> {
    let value = uri.strip_prefix(SESSION_PREFIX)?;
    let (session_id, child) = value.split_once(&format!("/{kind}/"))?;
    (!session_id.is_empty()
        && !child.is_empty()
        && !session_id.contains('/')
        && !child.contains('/'))
    .then_some((session_id, child))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_canonical_resource_shapes() {
        let session_id = LiveSessionId::new("session-1").unwrap();
        let camera_id = LiveCameraId::new("camera-1").unwrap();
        let stream_id = LiveViewId::new("stream-1").unwrap();

        assert_eq!(
            parse_session(&session(&session_id)),
            Some(session_id.clone())
        );
        assert_eq!(parse_scene(&scene(&session_id)), Some(session_id.clone()));
        assert_eq!(
            parse_pose_source(&pose_source(&session_id)),
            Some(session_id.clone())
        );
        assert_eq!(
            parse_camera(&camera(&session_id, &camera_id)),
            Some((session_id.clone(), camera_id))
        );
        assert_eq!(
            parse_stream(&stream(&session_id, &stream_id)),
            Some((session_id, stream_id))
        );
        assert!(parse_session("simulation-view://session/a/scene").is_none());
        assert!(parse_camera("simulation-view://session/a/camera/b/tail").is_none());
    }
}
