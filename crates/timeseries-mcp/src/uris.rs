use veoveo_mcp_contract::ServerResourceUris;

pub const ARTIFACT_TEMPLATE: &str = "timeseries://artifact/{sha256}";
pub const USAGE_ROOT_URI: &str = "timeseries://usage";
pub const USAGE_TASK_TEMPLATE: &str = "timeseries://usage/task/{task_id}";

fn timeseries_uris() -> ServerResourceUris {
    ServerResourceUris::new("timeseries")
}

pub fn artifact_uri(sha256: &str) -> String {
    timeseries_uris().artifact_uri(sha256)
}

pub fn usage_task_uri(task_id: &str) -> String {
    timeseries_uris().usage_task_uri(task_id)
}

pub fn parse_artifact_uri(uri: &str) -> Option<&str> {
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
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let uri = artifact_uri(sha);
        assert_eq!(uri, format!("timeseries://artifact/{sha}"));
        assert_eq!(parse_artifact_uri(&uri), Some(sha));
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
