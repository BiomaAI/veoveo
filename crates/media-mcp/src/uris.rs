//! `media://` URI scheme for this server's MCP resources.
//!
//! - `media://models`                  — compact model catalog index
//! - `media://model/{model_id}`        — full schema + pricing for one model
//! - `media://prediction/{id}`         — live prediction state (subscribable)
//! - `media://artifact/{artifact_id}`  — server-owned artifact metadata/content
//! - `media://usage/task/{task_id}`    — task usage estimates/actuals

use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

pub const MODELS_URI: &str = "media://models";
pub const MODEL_TEMPLATE: &str = "media://model/{model_id}";
pub const PREDICTION_TEMPLATE: &str = "media://prediction/{id}";
pub const ARTIFACT_TEMPLATE: &str = "media://artifact/{artifact_id}";
pub const USAGE_ROOT_URI: &str = "media://usage";
pub const USAGE_TASK_TEMPLATE: &str = "media://usage/task/{task_id}";

fn media_uris() -> ServerResourceUris {
    ServerResourceUris::new("media")
}

pub fn model_uri(model_id: &str) -> String {
    media_uris().model_uri(model_id)
}

pub fn prediction_uri(id: &str) -> String {
    media_uris().prediction_uri(id)
}

pub fn artifact_uri(artifact_id: ArtifactId) -> String {
    media_uris().artifact_uri(artifact_id)
}

pub fn usage_task_uri(task_id: &str) -> String {
    media_uris().usage_task_uri(task_id)
}

/// Parse a `media://model/{model_id}` URI. Model ids contain slashes
/// (e.g. `openai/gpt-image-2/edit`), so everything after the prefix is the id.
pub fn parse_model_uri(uri: &str) -> Option<&str> {
    media_uris().parse_model_uri(uri)
}

pub fn parse_prediction_uri(uri: &str) -> Option<&str> {
    media_uris().parse_prediction_uri(uri)
}

pub fn parse_artifact_uri(uri: &str) -> Option<ArtifactId> {
    media_uris().parse_artifact_uri(uri)
}

pub fn parse_usage_task_uri(uri: &str) -> Option<&str> {
    media_uris().parse_usage_task_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_uri_round_trip() {
        let uri = model_uri("openai/gpt-image-2/edit");
        assert_eq!(uri, "media://model/openai/gpt-image-2/edit");
        assert_eq!(parse_model_uri(&uri), Some("openai/gpt-image-2/edit"));
    }

    #[test]
    fn prediction_uri_round_trip() {
        let uri = prediction_uri("abc123");
        assert_eq!(parse_prediction_uri(&uri), Some("abc123"));
        assert_eq!(parse_prediction_uri("media://prediction/a/b"), None);
        assert_eq!(parse_model_uri("media://models"), None);
    }

    #[test]
    fn artifact_uri_round_trip() {
        let artifact_id = ArtifactId::new();
        let uri = artifact_uri(artifact_id);
        assert_eq!(uri, format!("media://artifact/{artifact_id}"));
        assert_eq!(parse_artifact_uri(&uri), Some(artifact_id));
        assert_eq!(parse_artifact_uri("media://artifact/not-a-sha"), None);
    }

    #[test]
    fn usage_task_uri_round_trip() {
        let uri = usage_task_uri("task-1");
        assert_eq!(uri, "media://usage/task/task-1");
        assert_eq!(parse_usage_task_uri(&uri), Some("task-1"));
        assert_eq!(parse_usage_task_uri("media://usage/task/a/b"), None);
    }
}
