use veoveo_mcp_contract::ServerResourceUris;

pub const DBS_ROOT_URI: &str = "duckdb://dbs";
pub const DB_TEMPLATE: &str = "duckdb://db/{db_id}";
pub const ARTIFACT_TEMPLATE: &str = "duckdb://artifact/{sha256}";
pub const USAGE_ROOT_URI: &str = "duckdb://usage";
pub const USAGE_TASK_TEMPLATE: &str = "duckdb://usage/task/{task_id}";

fn duckdb_uris() -> ServerResourceUris {
    ServerResourceUris::new("duckdb")
}

pub fn db_uri(db_id: &str) -> String {
    format!("duckdb://db/{db_id}")
}

pub fn parse_db_uri(uri: &str) -> Option<&str> {
    let rest = uri.strip_prefix("duckdb://db/")?;
    if rest.is_empty() || rest.contains('/') {
        None
    } else {
        Some(rest)
    }
}

pub fn artifact_uri(sha256: &str) -> String {
    duckdb_uris().artifact_uri(sha256)
}

pub fn usage_task_uri(task_id: &str) -> String {
    duckdb_uris().usage_task_uri(task_id)
}

pub fn parse_artifact_uri(uri: &str) -> Option<&str> {
    duckdb_uris().parse_artifact_uri(uri)
}

pub fn parse_usage_task_uri(uri: &str) -> Option<&str> {
    duckdb_uris().parse_usage_task_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_uri_round_trips() {
        let uri = db_uri("robot_metrics");
        assert_eq!(uri, "duckdb://db/robot_metrics");
        assert_eq!(parse_db_uri(&uri), Some("robot_metrics"));
        assert_eq!(parse_db_uri("duckdb://db/"), None);
        assert_eq!(parse_db_uri("duckdb://db/a/b"), None);
        assert_eq!(parse_db_uri("duckdb://dbs"), None);
    }

    #[test]
    fn artifact_uri_round_trips() {
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let uri = artifact_uri(sha);
        assert_eq!(uri, format!("duckdb://artifact/{sha}"));
        assert_eq!(parse_artifact_uri(&uri), Some(sha));
    }
}
