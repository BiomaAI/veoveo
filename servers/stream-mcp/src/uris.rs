use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

/// Well-known surface roots (contract C18, C19). These literals must match
/// `veoveo_mcp_contract::ServerResourceUris::new("stream")`; a unit test below
/// pins that equivalence.
pub const DOCS_URI: &str = "stream://docs";
pub const CONTRACT_URI: &str = "stream://contract";
pub const DOC_TEMPLATE: &str = "stream://docs/{doc_id}";

pub const PIPELINES_URI: &str = "stream://pipelines";
pub const PIPELINE_TEMPLATE: &str = "stream://pipeline/{pipeline_id}";
pub const MODELS_URI: &str = "stream://models";
pub const MODEL_TEMPLATE: &str = "stream://model/{model_id}";
pub const RUNS_URI: &str = "stream://runs";
pub const RUN_TEMPLATE: &str = "stream://run/{run_id}";
pub const RUN_RESULTS_TEMPLATE: &str = "stream://run/{run_id}/results";
pub const SESSIONS_URI: &str = "stream://sessions";
pub const SESSION_TEMPLATE: &str = "stream://session/{session_id}";
pub const SESSION_RESULTS_TEMPLATE: &str = "stream://session/{session_id}/results";
pub const SESSION_PREVIEW_TEMPLATE: &str = "stream://session/{session_id}/preview";
pub const ARTIFACT_TEMPLATE: &str = "stream://artifact/{artifact_id}";
pub const LIVE_APP_URI: &str = "ui://stream/live.html";

fn server_uris() -> ServerResourceUris {
    ServerResourceUris::new("stream")
}

pub fn doc_uri(doc_id: &str) -> String {
    server_uris().doc_uri(doc_id)
}

pub fn parse_doc(uri: &str) -> Option<&str> {
    veoveo_mcp_contract::parse_server_doc_uri("stream", uri)
}

pub fn pipeline_uri(id: &str) -> String {
    format!("stream://pipeline/{id}")
}

pub fn model_uri(id: &str) -> String {
    format!("stream://model/{id}")
}

pub fn run_uri(id: &str) -> String {
    format!("stream://run/{id}")
}

pub fn results_uri(id: &str) -> String {
    format!("stream://run/{id}/results")
}

pub fn session_uri(id: &str) -> String {
    format!("stream://session/{id}")
}

pub fn session_results_uri(id: &str) -> String {
    format!("stream://session/{id}/results")
}

pub fn session_preview_uri(id: &str) -> String {
    format!("stream://session/{id}/preview")
}

pub fn parse_pipeline_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "stream://pipeline/")
}

pub fn parse_model_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "stream://model/")
}

pub fn parse_run_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "stream://run/")
}

pub fn parse_results_uri(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("stream://run/")?;
    let value = value.strip_suffix("/results")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

pub fn parse_session_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "stream://session/")
}

pub fn parse_session_results_uri(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("stream://session/")?;
    let value = value.strip_suffix("/results")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

pub fn parse_session_preview_uri(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("stream://session/")?;
    let value = value.strip_suffix("/preview")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

pub fn artifact_uri(id: ArtifactId) -> String {
    server_uris().artifact_uri(id)
}

pub fn parse_artifact_uri(uri: &str) -> Option<ArtifactId> {
    server_uris().parse_artifact_uri(uri)
}

fn parse_single<'a>(uri: &'a str, prefix: &str) -> Option<&'a str> {
    let value = uri.strip_prefix(prefix)?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_uris_match_the_shared_contract_conventions() {
        let conventions = server_uris();
        assert_eq!(DOCS_URI, conventions.docs_root_uri());
        assert_eq!(CONTRACT_URI, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(doc_uri("agents"), "stream://docs/agents");
        assert_eq!(parse_doc("stream://docs/agents"), Some("agents"));
        assert_eq!(parse_doc("stream://docs"), None);
        assert_eq!(parse_doc("stream://docs/agents/extra"), None);
    }

    #[test]
    fn run_and_session_uris_are_unambiguous() {
        assert_eq!(parse_run_uri(&run_uri("task-1")), Some("task-1"));
        assert_eq!(parse_results_uri(&results_uri("task-1")), Some("task-1"));
        assert_eq!(parse_run_uri(&results_uri("task-1")), None);
        assert_eq!(parse_session_uri(&session_uri("live-1")), Some("live-1"));
        assert_eq!(
            parse_session_results_uri(&session_results_uri("live-1")),
            Some("live-1")
        );
        assert_eq!(parse_session_uri(&session_results_uri("live-1")), None);
        assert_eq!(
            parse_session_preview_uri(&session_preview_uri("live-1")),
            Some("live-1")
        );
    }
}
