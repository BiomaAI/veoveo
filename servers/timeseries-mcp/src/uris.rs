use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

/// Well-known surface roots (contract C18, C19). These literals must match
/// `ServerResourceUris::new("timeseries")`; a unit test below pins that
/// equivalence.
pub const DOCS_URI: &str = "timeseries://docs";
pub const CONTRACT_URI: &str = "timeseries://contract";
pub const DOC_TEMPLATE: &str = "timeseries://docs/{doc_id}";
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

pub fn doc_uri(doc_id: &str) -> String {
    timeseries_uris().doc_uri(doc_id)
}

pub fn parse_doc(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("timeseries://docs/")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
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
    fn well_known_uris_match_the_shared_conventions() {
        let conventions = timeseries_uris();
        assert_eq!(DOCS_URI, conventions.docs_root_uri());
        assert_eq!(CONTRACT_URI, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(doc_uri("agents"), "timeseries://docs/agents");
        assert_eq!(parse_doc("timeseries://docs/agents"), Some("agents"));
        assert_eq!(parse_doc("timeseries://docs"), None);
        assert_eq!(parse_doc("timeseries://docs/agents/extra"), None);
    }

    #[test]
    fn usage_task_uri_round_trips() {
        let uri = usage_task_uri("task-1");
        assert_eq!(uri, "timeseries://usage/task/task-1");
        assert_eq!(parse_usage_task_uri(&uri), Some("task-1"));
        assert_eq!(parse_usage_task_uri("timeseries://usage/task/a/b"), None);
    }
}
