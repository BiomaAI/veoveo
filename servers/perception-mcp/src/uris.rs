use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

pub const PIPELINES_URI: &str = "perception://pipelines";
pub const PIPELINE_TEMPLATE: &str = "perception://pipeline/{pipeline_id}";
pub const MODELS_URI: &str = "perception://models";
pub const MODEL_TEMPLATE: &str = "perception://model/{model_id}";
pub const ANALYSES_URI: &str = "perception://analyses";
pub const ANALYSIS_TEMPLATE: &str = "perception://analysis/{analysis_id}";
pub const RESULTS_TEMPLATE: &str = "perception://analysis/{analysis_id}/results";
pub const ARTIFACT_TEMPLATE: &str = "perception://artifact/{artifact_id}";

fn server_uris() -> ServerResourceUris {
    ServerResourceUris::new("perception")
}

pub fn pipeline_uri(id: &str) -> String {
    format!("perception://pipeline/{id}")
}

pub fn model_uri(id: &str) -> String {
    format!("perception://model/{id}")
}

pub fn analysis_uri(id: &str) -> String {
    format!("perception://analysis/{id}")
}

pub fn results_uri(id: &str) -> String {
    format!("perception://analysis/{id}/results")
}

pub fn parse_pipeline_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "perception://pipeline/")
}

pub fn parse_model_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "perception://model/")
}

pub fn parse_analysis_uri(uri: &str) -> Option<&str> {
    parse_single(uri, "perception://analysis/")
}

pub fn parse_results_uri(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("perception://analysis/")?;
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
    fn analysis_uris_are_unambiguous() {
        assert_eq!(parse_analysis_uri(&analysis_uri("task-1")), Some("task-1"));
        assert_eq!(parse_results_uri(&results_uri("task-1")), Some("task-1"));
        assert_eq!(parse_analysis_uri(&results_uri("task-1")), None);
    }
}
