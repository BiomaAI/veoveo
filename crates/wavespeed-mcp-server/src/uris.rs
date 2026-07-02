//! `wavespeed://` URI scheme shared by server and client.
//!
//! - `wavespeed://models`                  — compact model catalog index
//! - `wavespeed://model/{model_id}`        — full schema + pricing for one model
//! - `wavespeed://prediction/{id}`         — live prediction state (subscribable)

use mcp_foundation::ProviderUris;

pub const MODELS_URI: &str = "wavespeed://models";
pub const MODEL_TEMPLATE: &str = "wavespeed://model/{model_id}";
pub const PREDICTION_TEMPLATE: &str = "wavespeed://prediction/{id}";

const WAVESPEED_URIS: ProviderUris = ProviderUris::new("wavespeed");

pub fn model_uri(model_id: &str) -> String {
    WAVESPEED_URIS.model_uri(model_id)
}

pub fn prediction_uri(id: &str) -> String {
    WAVESPEED_URIS.prediction_uri(id)
}

/// Parse a `wavespeed://model/{model_id}` URI. Model ids contain slashes
/// (e.g. `openai/gpt-image-2/edit`), so everything after the prefix is the id.
pub fn parse_model_uri(uri: &str) -> Option<&str> {
    WAVESPEED_URIS.parse_model_uri(uri)
}

pub fn parse_prediction_uri(uri: &str) -> Option<&str> {
    WAVESPEED_URIS.parse_prediction_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_uri_round_trip() {
        let uri = model_uri("openai/gpt-image-2/edit");
        assert_eq!(uri, "wavespeed://model/openai/gpt-image-2/edit");
        assert_eq!(parse_model_uri(&uri), Some("openai/gpt-image-2/edit"));
    }

    #[test]
    fn prediction_uri_round_trip() {
        let uri = prediction_uri("abc123");
        assert_eq!(parse_prediction_uri(&uri), Some("abc123"));
        assert_eq!(parse_prediction_uri("wavespeed://prediction/a/b"), None);
        assert_eq!(parse_model_uri("wavespeed://models"), None);
    }
}
