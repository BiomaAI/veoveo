use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

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
    fn analysis_uris_are_unambiguous() {
        assert_eq!(parse_analysis_uri(&analysis_uri("task-1")), Some("task-1"));
        assert_eq!(parse_results_uri(&results_uri("task-1")), Some("task-1"));
        assert_eq!(parse_analysis_uri(&results_uri("task-1")), None);
    }
}
