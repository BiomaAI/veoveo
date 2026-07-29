use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

/// Well-known surface roots (contract C18, C19). These literals must match
/// `veoveo_mcp_contract::ServerResourceUris::new("reason")`; a unit test below
/// pins that equivalence.
pub const DOCS_URI: &str = "reason://docs";
pub const CONTRACT_URI: &str = "reason://contract";
pub const DOC_TEMPLATE: &str = "reason://docs/{doc_id}";

pub const PIPELINES_URI: &str = "reason://pipelines";
pub const PIPELINE_TEMPLATE: &str = "reason://pipeline/{pipeline_id}";
pub const MODELS_URI: &str = "reason://models";
pub const MODEL_TEMPLATE: &str = "reason://model/{model_id}";
pub const ANALYSES_URI: &str = "reason://analyses";
pub const ANALYSIS_TEMPLATE: &str = "reason://analysis/{analysis_id}";
pub const RESULTS_TEMPLATE: &str = "reason://analysis/{analysis_id}/results";
pub const ARTIFACT_TEMPLATE: &str = "reason://artifact/{artifact_id}";

fn server_uris() -> ServerResourceUris {
    ServerResourceUris::new("reason")
}

pub fn doc_uri(doc_id: &str) -> String {
    server_uris().doc_uri(doc_id)
}

pub fn parse_doc(uri: &str) -> Option<&str> {
    veoveo_mcp_contract::parse_server_doc_uri("reason", uri)
}

pub fn pipeline_uri(id: &str) -> String {
    format!("reason://pipeline/{id}")
}

pub fn model_uri(id: &str) -> String {
    format!("reason://model/{id}")
}

pub fn analysis_uri(id: &str) -> String {
    format!("reason://analysis/{id}")
}

pub fn results_uri(id: &str) -> String {
    format!("reason://analysis/{id}/results")
}

pub fn parse_pipeline_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "reason://pipeline/")
}

pub fn parse_model_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "reason://model/")
}

pub fn parse_analysis_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "reason://analysis/")
}

pub fn parse_results_uri(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("reason://analysis/")?;
    let value = value.strip_suffix("/results")?;
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
        assert_eq!(doc_uri("agents"), "reason://docs/agents");
        assert_eq!(parse_doc("reason://docs/agents"), Some("agents"));
        assert_eq!(parse_doc("reason://docs"), None);
        assert_eq!(parse_doc("reason://docs/agents/extra"), None);
    }

    #[test]
    fn analysis_uris_are_unambiguous() {
        assert_eq!(parse_analysis_uri(&analysis_uri("task-1")), Some("task-1"));
        assert_eq!(parse_results_uri(&results_uri("task-1")), Some("task-1"));
        assert_eq!(parse_analysis_uri(&results_uri("task-1")), None);
    }
}
