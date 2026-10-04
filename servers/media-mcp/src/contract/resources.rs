//! Complete hosted route vocabulary, independent of MCP and service dependencies.
use super::MediaArtifactUri;
use super::{
    MediaGenerationUri, MediaModelUri, MediaPredictionIndexUri, MediaPredictionUri,
    MediaTaskUsageUri, MediaUsageIndexUri,
};
use crate::uris;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaDocument {
    Agents,
    Design,
}
impl MediaDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
    pub fn parse(value: &str) -> Result<Self, MediaResourceError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(MediaResourceError),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub enum MediaResource {
    Models,
    Model(MediaModelUri),
    Predictions(MediaPredictionIndexUri),
    Prediction(MediaPredictionUri),
    Generation(MediaGenerationUri),
    Docs,
    Document(MediaDocument),
    Contract,
    StudioApp,
    Usage(MediaUsageIndexUri),
    TaskUsage(MediaTaskUsageUri),
    Artifact(MediaArtifactUri),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Media resource URI")]
pub struct MediaResourceError;

impl MediaResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MediaResourceError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MediaResourceError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        let segments = path.iter().map(|part| part.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.scheme(), parts.authority(), segments.as_slice()) {
            ("media", "usage", []) => {
                Self::Usage(MediaUsageIndexUri::parse(value).map_err(|_| MediaResourceError)?)
            }
            ("media", "usage", ["task", _]) => {
                Self::TaskUsage(MediaTaskUsageUri::parse(value).map_err(|_| MediaResourceError)?)
            }
            ("media", "predictions", []) => Self::Predictions(
                MediaPredictionIndexUri::parse(value).map_err(|_| MediaResourceError)?,
            ),
            ("media", "prediction", [_]) => {
                Self::Prediction(MediaPredictionUri::parse(value).map_err(|_| MediaResourceError)?)
            }
            ("media", "prediction", [_, "result"]) => {
                Self::Generation(MediaGenerationUri::parse(value).map_err(|_| MediaResourceError)?)
            }
            ("media", "model", _) => {
                Self::Model(MediaModelUri::parse(value).map_err(|_| MediaResourceError)?)
            }
            ("media", "models", []) if !parts.has_query() => Self::Models,
            _ if parts.has_query() => return Err(MediaResourceError),
            ("media", "artifact", [_]) => {
                Self::Artifact(MediaArtifactUri::parse(value).map_err(|_| MediaResourceError)?)
            }
            ("media", "docs", []) => Self::Docs,
            ("media", "docs", [id]) => Self::Document(MediaDocument::parse(id)?),
            ("media", "contract", []) => Self::Contract,
            ("ui", "media", ["studio.html"]) => Self::StudioApp,
            _ => return Err(MediaResourceError),
        };
        if resource.to_uri()?.as_str() != value {
            return Err(MediaResourceError);
        }
        Ok(resource)
    }
}
impl ResourceAddress for MediaResource {
    type Error = MediaResourceError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        match self {
            Self::Models => {
                ResourceUri::new(crate::uris::MODELS_URI).map_err(|_| MediaResourceError)
            }
            Self::Model(uri) => uri.to_uri().map_err(|_| MediaResourceError),
            Self::Predictions(uri) => uri.to_uri().map_err(|_| MediaResourceError),
            Self::Prediction(uri) => uri.to_uri().map_err(|_| MediaResourceError),
            Self::Generation(uri) => uri.to_uri().map_err(|_| MediaResourceError),
            Self::Usage(uri) => uri.to_uri().map_err(|_| MediaResourceError),
            Self::TaskUsage(uri) => uri.to_uri().map_err(|_| MediaResourceError),
            Self::Artifact(id) => id.to_uri().map_err(|_| MediaResourceError),
            Self::Document(doc) => ResourceUriBuilder::new(uris::DOCS_URI)
                .map_err(|_| MediaResourceError)?
                .segment(UriSegment::new(doc.as_str()).map_err(|_| MediaResourceError)?)
                .build()
                .map_err(|_| MediaResourceError),
            Self::Docs => ResourceUri::new(uris::DOCS_URI).map_err(|_| MediaResourceError),
            Self::Contract => ResourceUri::new(uris::CONTRACT_URI).map_err(|_| MediaResourceError),
            Self::StudioApp => {
                ResourceUri::new(uris::STUDIO_APP_URI).map_err(|_| MediaResourceError)
            }
        }
    }
}
impl TryFrom<String> for MediaResource {
    type Error = MediaResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MediaResource> for String {
    fn from(value: MediaResource) -> Self {
        value.to_uri().expect("admitted Media address").to_string()
    }
}
