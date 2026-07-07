use veoveo_mcp_contract::ServerResourceUris;

pub const ARTIFACT_TEMPLATE: &str = "optimization://artifact/{sha256}";
pub const USAGE_ROOT_URI: &str = "optimization://usage";
pub const USAGE_TASK_TEMPLATE: &str = "optimization://usage/task/{task_id}";

fn optimization_uris() -> ServerResourceUris {
    ServerResourceUris::new("optimization")
}

pub fn artifact_uri(sha256: &str) -> String {
    optimization_uris().artifact_uri(sha256)
}

pub fn usage_task_uri(task_id: &str) -> String {
    optimization_uris().usage_task_uri(task_id)
}

pub fn parse_artifact_uri(uri: &str) -> Option<&str> {
    optimization_uris().parse_artifact_uri(uri)
}

pub fn parse_usage_task_uri(uri: &str) -> Option<&str> {
    optimization_uris().parse_usage_task_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_uri_round_trips() {
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let uri = artifact_uri(sha);
        assert_eq!(uri, format!("optimization://artifact/{sha}"));
        assert_eq!(parse_artifact_uri(&uri), Some(sha));
        assert_eq!(parse_artifact_uri("optimization://artifact/nope"), None);
    }

    #[test]
    fn usage_task_uri_round_trips() {
        let uri = usage_task_uri("task-1");
        assert_eq!(uri, "optimization://usage/task/task-1");
        assert_eq!(parse_usage_task_uri(&uri), Some("task-1"));
        assert_eq!(parse_usage_task_uri("optimization://usage/task/a/b"), None);
    }
}
