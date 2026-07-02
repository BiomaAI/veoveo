/// Provider URI conventions for MCP resources.
///
/// A provider owns a URI scheme, then exposes a stable catalog resource,
/// per-model resources, per-prediction resources, artifacts, and usage records.
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

    pub fn artifact_template(&self) -> String {
        format!("{}://artifact/{{sha256}}", self.scheme)
    }

    pub fn usage_root_uri(&self) -> String {
        format!("{}://usage", self.scheme)
    }

    pub fn usage_task_template(&self) -> String {
        format!("{}://usage/task/{{task_id}}", self.scheme)
    }

    pub fn model_uri(&self, model_id: &str) -> String {
        format!("{}://model/{model_id}", self.scheme)
    }

    pub fn prediction_uri(&self, id: &str) -> String {
        format!("{}://prediction/{id}", self.scheme)
    }

    pub fn artifact_uri(&self, sha256: &str) -> String {
        format!("{}://artifact/{sha256}", self.scheme)
    }

    pub fn usage_task_uri(&self, task_id: &str) -> String {
        format!("{}://usage/task/{task_id}", self.scheme)
    }

    pub fn parse_model_uri<'a>(&self, uri: &'a str) -> Option<&'a str> {
        uri.strip_prefix(&format!("{}://model/", self.scheme))
            .filter(|s| !s.is_empty())
    }

    pub fn parse_prediction_uri<'a>(&self, uri: &'a str) -> Option<&'a str> {
        uri.strip_prefix(&format!("{}://prediction/", self.scheme))
            .filter(|s| !s.is_empty() && !s.contains('/'))
    }

    pub fn parse_artifact_uri<'a>(&self, uri: &'a str) -> Option<&'a str> {
        uri.strip_prefix(&format!("{}://artifact/", self.scheme))
            .filter(|s| is_sha256(s))
    }

    pub fn parse_usage_task_uri<'a>(&self, uri: &'a str) -> Option<&'a str> {
        uri.strip_prefix(&format!("{}://usage/task/", self.scheme))
            .filter(|s| !s.is_empty() && !s.contains('/'))
    }
}

pub fn artifact_object_key(sha256: &str) -> String {
    format!("artifact/{sha256}")
}

pub fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
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
        assert_eq!(uris.artifact_template(), "example://artifact/{sha256}");
        assert_eq!(uris.usage_root_uri(), "example://usage");
        assert_eq!(uris.usage_task_template(), "example://usage/task/{task_id}");
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

        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(uris.artifact_uri(sha), format!("example://artifact/{sha}"));
        assert_eq!(uris.parse_artifact_uri(&uris.artifact_uri(sha)), Some(sha));
        assert_eq!(uris.parse_artifact_uri("example://artifact/not-sha"), None);
        assert_eq!(artifact_object_key(sha), format!("artifact/{sha}"));

        assert_eq!(uris.usage_task_uri("task-1"), "example://usage/task/task-1");
        assert_eq!(
            uris.parse_usage_task_uri("example://usage/task/task-1"),
            Some("task-1")
        );
    }
}
