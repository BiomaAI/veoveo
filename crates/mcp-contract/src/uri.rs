use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

/// Strongly typed resource shapes shared by Veoveo provider-style MCP servers.
///
/// The scheme remains server-owned. The gateway uses this type to project
/// task-addressed resources without parsing resource strings ad hoc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ProviderResourceUri {
    Models { scheme: String },
    Model { scheme: String, model_id: String },
    Prediction { scheme: String, id: String },
    Artifact { scheme: String, sha256: String },
    UsageRoot { scheme: String },
    UsageTask { scheme: String, task_id: String },
    Other { scheme: String, path: String },
}

impl ProviderResourceUri {
    pub fn parse(uri: &str) -> Result<Self, ProviderResourceUriError> {
        let (scheme, path) = uri
            .split_once("://")
            .ok_or_else(|| ProviderResourceUriError::new(uri, "must include a URI scheme"))?;
        validate_scheme(scheme)?;
        validate_path(uri, path)?;
        Ok(match path {
            "models" => Self::Models {
                scheme: scheme.to_string(),
            },
            "usage" => Self::UsageRoot {
                scheme: scheme.to_string(),
            },
            path if path.starts_with("model/") => {
                let model_id = path.trim_start_matches("model/");
                if model_id.is_empty() {
                    return Err(ProviderResourceUriError::new(
                        uri,
                        "model id must not be empty",
                    ));
                }
                Self::Model {
                    scheme: scheme.to_string(),
                    model_id: model_id.to_string(),
                }
            }
            path if path.starts_with("prediction/") => {
                let id = path.trim_start_matches("prediction/");
                if id.is_empty() || id.contains('/') {
                    return Err(ProviderResourceUriError::new(
                        uri,
                        "prediction id must be one path segment",
                    ));
                }
                Self::Prediction {
                    scheme: scheme.to_string(),
                    id: id.to_string(),
                }
            }
            path if path.starts_with("artifact/") => {
                let sha256 = path.trim_start_matches("artifact/");
                if !is_sha256(sha256) {
                    return Err(ProviderResourceUriError::new(
                        uri,
                        "artifact id must be a sha256 hex digest",
                    ));
                }
                Self::Artifact {
                    scheme: scheme.to_string(),
                    sha256: sha256.to_string(),
                }
            }
            path if path.starts_with("usage/task/") => {
                let task_id = path.trim_start_matches("usage/task/");
                if task_id.is_empty() || task_id.contains('/') {
                    return Err(ProviderResourceUriError::new(
                        uri,
                        "usage task id must be one path segment",
                    ));
                }
                Self::UsageTask {
                    scheme: scheme.to_string(),
                    task_id: task_id.to_string(),
                }
            }
            path => Self::Other {
                scheme: scheme.to_string(),
                path: path.to_string(),
            },
        })
    }

    pub fn scheme(&self) -> &str {
        match self {
            Self::Models { scheme }
            | Self::Model { scheme, .. }
            | Self::Prediction { scheme, .. }
            | Self::Artifact { scheme, .. }
            | Self::UsageRoot { scheme }
            | Self::UsageTask { scheme, .. }
            | Self::Other { scheme, .. } => scheme,
        }
    }

    pub fn usage_task_id(&self) -> Option<&str> {
        match self {
            Self::UsageTask { task_id, .. } => Some(task_id),
            _ => None,
        }
    }

    pub fn with_usage_task_id(
        &self,
        task_id: impl Into<String>,
    ) -> Result<Self, ProviderResourceUriError> {
        let task_id = task_id.into();
        validate_one_segment(&task_id, "usage task id")?;
        match self {
            Self::UsageTask { scheme, .. } => Ok(Self::UsageTask {
                scheme: scheme.clone(),
                task_id,
            }),
            _ => Err(ProviderResourceUriError::new(
                self.to_string(),
                "resource is not task-addressed usage",
            )),
        }
    }
}

impl fmt::Display for ProviderResourceUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Models { scheme } => write!(f, "{scheme}://models"),
            Self::Model { scheme, model_id } => write!(f, "{scheme}://model/{model_id}"),
            Self::Prediction { scheme, id } => write!(f, "{scheme}://prediction/{id}"),
            Self::Artifact { scheme, sha256 } => write!(f, "{scheme}://artifact/{sha256}"),
            Self::UsageRoot { scheme } => write!(f, "{scheme}://usage"),
            Self::UsageTask { scheme, task_id } => write!(f, "{scheme}://usage/task/{task_id}"),
            Self::Other { scheme, path } => write!(f, "{scheme}://{path}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResourceUriError {
    value: String,
    message: &'static str,
}

impl ProviderResourceUriError {
    fn new(value: impl Into<String>, message: &'static str) -> Self {
        Self {
            value: value.into(),
            message,
        }
    }
}

impl fmt::Display for ProviderResourceUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` {}", self.value, self.message)
    }
}

impl Error for ProviderResourceUriError {}

fn validate_scheme(scheme: &str) -> Result<(), ProviderResourceUriError> {
    let mut bytes = scheme.bytes();
    let Some(first) = bytes.next() else {
        return Err(ProviderResourceUriError::new(
            scheme,
            "scheme must not be empty",
        ));
    };
    if !first.is_ascii_lowercase()
        || !bytes.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'+' || b == b'-' || b == b'.'
        })
    {
        return Err(ProviderResourceUriError::new(
            scheme,
            "scheme must follow lowercase URI scheme syntax",
        ));
    }
    Ok(())
}

fn validate_path(uri: &str, path: &str) -> Result<(), ProviderResourceUriError> {
    if path.is_empty() || path.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(ProviderResourceUriError::new(
            uri,
            "path must be non-empty and contain no whitespace or control characters",
        ));
    }
    Ok(())
}

fn validate_one_segment(value: &str, label: &'static str) -> Result<(), ProviderResourceUriError> {
    if value.is_empty()
        || value.contains('/')
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(ProviderResourceUriError::new(
            value,
            match label {
                "usage task id" => "usage task id must be one non-empty path segment",
                _ => "value must be one non-empty path segment",
            },
        ));
    }
    Ok(())
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

    #[test]
    fn provider_resource_uri_parses_standard_shapes() {
        assert_eq!(
            ProviderResourceUri::parse("media://models").unwrap(),
            ProviderResourceUri::Models {
                scheme: "media".into()
            }
        );
        assert_eq!(
            ProviderResourceUri::parse("media://model/provider/model").unwrap(),
            ProviderResourceUri::Model {
                scheme: "media".into(),
                model_id: "provider/model".into()
            }
        );
        assert_eq!(
            ProviderResourceUri::parse(
                "media://artifact/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            )
            .unwrap()
            .to_string(),
            "media://artifact/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            ProviderResourceUri::parse("media://usage/task/gateway-task-1")
                .unwrap()
                .usage_task_id(),
            Some("gateway-task-1")
        );
        assert_eq!(
            ProviderResourceUri::parse("media://custom/path").unwrap(),
            ProviderResourceUri::Other {
                scheme: "media".into(),
                path: "custom/path".into()
            }
        );
    }

    #[test]
    fn provider_resource_uri_rewrites_usage_task_structurally() {
        let uri = ProviderResourceUri::parse("media://usage/task/gateway-task-1").unwrap();
        let rewritten = uri.with_usage_task_id("upstream-task-1").unwrap();
        assert_eq!(rewritten.to_string(), "media://usage/task/upstream-task-1");
        assert!(
            ProviderResourceUri::parse("media://models")
                .unwrap()
                .with_usage_task_id("task-1")
                .is_err()
        );
        assert!(uri.with_usage_task_id("bad/task").is_err());
    }
}
