//! `media://` URI scheme for this server's MCP resources.
//!
//! - `media://models`                  — compact model catalog index
//! - `media://model/{model_id}`        — full schema + pricing for one model
//! - `media://prediction/{id}`         — live prediction state (subscribable)

use veoveo_mcp_contract::ProviderUris;

pub const MODELS_URI: &str = "media://models";
pub const MODEL_TEMPLATE: &str = "media://model/{model_id}";
pub const PREDICTION_TEMPLATE: &str = "media://prediction/{id}";

fn media_uris() -> ProviderUris {
    ProviderUris::new("media")
}

pub fn model_uri(model_id: &str) -> String {
    media_uris().model_uri(model_id)
}

pub fn prediction_uri(id: &str) -> String {
    media_uris().prediction_uri(id)
}

/// Parse a `media://model/{model_id}` URI. Model ids contain slashes
/// (e.g. `openai/gpt-image-2/edit`), so everything after the prefix is the id.
pub fn parse_model_uri(uri: &str) -> Option<&str> {
    media_uris().parse_model_uri(uri)
}

pub fn parse_prediction_uri(uri: &str) -> Option<&str> {
    media_uris().parse_prediction_uri(uri)
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
}
