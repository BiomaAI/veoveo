/// Provider URI conventions for MCP resources.
///
/// A provider owns a URI scheme, then exposes a stable catalog resource,
/// per-model resources, and per-prediction resources.
#[derive(Debug, Clone)]
pub struct ProviderUris {
    scheme: String,
}

impl ProviderUris {
    pub fn new(scheme: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into(),
        }
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn models_uri(&self) -> String {
        format!("{}://models", self.scheme)
    }

    pub fn model_template(&self) -> String {
        format!("{}://model/{{model_id}}", self.scheme)
    }

    pub fn prediction_template(&self) -> String {
        format!("{}://prediction/{{id}}", self.scheme)
    }

    pub fn model_uri(&self, model_id: &str) -> String {
        format!("{}://model/{model_id}", self.scheme)
    }

    pub fn prediction_uri(&self, id: &str) -> String {
        format!("{}://prediction/{id}", self.scheme)
    }

    pub fn parse_model_uri<'a>(&self, uri: &'a str) -> Option<&'a str> {
        uri.strip_prefix(&format!("{}://model/", self.scheme))
            .filter(|s| !s.is_empty())
    }

    pub fn parse_prediction_uri<'a>(&self, uri: &'a str) -> Option<&'a str> {
        uri.strip_prefix(&format!("{}://prediction/", self.scheme))
            .filter(|s| !s.is_empty() && !s.contains('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_uri_conventions_round_trip() {
        let uris = ProviderUris::new("example");
        assert_eq!(uris.models_uri(), "example://models");
        assert_eq!(uris.model_template(), "example://model/{model_id}");
        assert_eq!(uris.prediction_template(), "example://prediction/{id}");
        assert_eq!(
            uris.model_uri("provider/model"),
            "example://model/provider/model"
        );
        assert_eq!(
            uris.parse_model_uri("example://model/provider/model"),
            Some("provider/model")
        );
        assert_eq!(uris.prediction_uri("abc123"), "example://prediction/abc123");
        assert_eq!(
            uris.parse_prediction_uri("example://prediction/abc123"),
            Some("abc123")
        );
        assert_eq!(uris.parse_prediction_uri("example://prediction/a/b"), None);
    }
}
