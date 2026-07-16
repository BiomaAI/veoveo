use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

pub const ARTIFACT_TEMPLATE: &str = "timeseries://artifact/{artifact_id}";
/// The forecast app view. The first path segment is the server slug; the
/// gateway's ServerOwned projection rewrites it to the mounted slug, so the
/// URI is stable end to end.
pub const FORECAST_APP_URI: &str = "ui://timeseries/forecast.html";
pub const USAGE_ROOT_URI: &str = "timeseries://usage";
pub const USAGE_TASK_TEMPLATE: &str = "timeseries://usage/task/{task_id}";

fn timeseries_uris() -> ServerResourceUris {
    ServerResourceUris::new("timeseries")
}

pub fn artifact_uri(artifact_id: ArtifactId) -> String {
    timeseries_uris().artifact_uri(artifact_id)
}

pub fn usage_task_uri(task_id: &str) -> String {
    timeseries_uris().usage_task_uri(task_id)
}

pub fn parse_artifact_uri(uri: &str) -> Option<ArtifactId> {
    timeseries_uris().parse_artifact_uri(uri)
}

pub fn parse_usage_task_uri(uri: &str) -> Option<&str> {
    timeseries_uris().parse_usage_task_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_uri_round_trips() {
        let artifact_id = ArtifactId::new();
        let uri = artifact_uri(artifact_id);
        assert_eq!(uri, format!("timeseries://artifact/{artifact_id}"));
        assert_eq!(parse_artifact_uri(&uri), Some(artifact_id));
        assert_eq!(parse_artifact_uri("timeseries://artifact/nope"), None);
    }

    #[test]
    fn usage_task_uri_round_trips() {
        let uri = usage_task_uri("task-1");
        assert_eq!(uri, "timeseries://usage/task/task-1");
        assert_eq!(parse_usage_task_uri(&uri), Some("task-1"));
        assert_eq!(parse_usage_task_uri("timeseries://usage/task/a/b"), None);
    }
}
