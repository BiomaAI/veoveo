//! Complete hosted route vocabulary, independent of MCP and service dependencies.
use super::{
    FrameOperationUri, FrameTaskUsageUri, FrameUsageIndexUri, FrameWorldRevisionUri, FrameWorldUri,
    FrameWorldsUri, WorldFrameUri,
};
use crate::uris;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FramesDocument {
    Agents,
    Design,
}
impl FramesDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
    pub fn parse(value: &str) -> Result<Self, FramesResourceError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(FramesResourceError),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub enum FramesResource {
    Docs,
    Document(FramesDocument),
    Contract,
    WorkspaceApp,
    Worlds(FrameWorldsUri),
    World(FrameWorldUri),
    Revision(FrameWorldRevisionUri),
    Frame(WorldFrameUri),
    Operation(FrameOperationUri),
    Usage(FrameUsageIndexUri),
    TaskUsage(FrameTaskUsageUri),
    Artifact(ArtifactId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Frames resource URI")]
pub struct FramesResourceError;

impl FramesResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, FramesResourceError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| FramesResourceError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        let segments = path.iter().map(|part| part.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.scheme(), parts.authority(), segments.as_slice()) {
            ("frames", "worlds", []) => {
                Self::Worlds(FrameWorldsUri::parse(value).map_err(|_| FramesResourceError)?)
            }
            ("frames", "world", [_]) => {
                Self::World(FrameWorldUri::parse(value).map_err(|_| FramesResourceError)?)
            }
            ("frames", "world", [_, "revision", _]) => Self::Revision(
                FrameWorldRevisionUri::parse(value).map_err(|_| FramesResourceError)?,
            ),
            ("frames", "world", [_, "revision", _, "frame", _]) => {
                Self::Frame(WorldFrameUri::parse(value).map_err(|_| FramesResourceError)?)
            }
            ("frames", "operation", [_]) => {
                Self::Operation(FrameOperationUri::parse(value).map_err(|_| FramesResourceError)?)
            }
            ("frames", "usage", []) => {
                Self::Usage(FrameUsageIndexUri::parse(value).map_err(|_| FramesResourceError)?)
            }
            ("frames", "usage", ["task", _]) => {
                Self::TaskUsage(FrameTaskUsageUri::parse(value).map_err(|_| FramesResourceError)?)
            }
            _ if parts.has_query() => return Err(FramesResourceError),
            ("frames", "artifact", [_]) => Self::Artifact(
                ArtifactUri::parse(value)
                    .map_err(|_| FramesResourceError)?
                    .artifact_id(),
            ),
            ("frames", "docs", []) => Self::Docs,
            ("frames", "docs", [id]) => Self::Document(FramesDocument::parse(id)?),
            ("frames", "contract", []) => Self::Contract,
            ("ui", "frames", ["workspace.html"]) => Self::WorkspaceApp,
            _ => return Err(FramesResourceError),
        };
        if resource.to_uri()?.as_str() != value {
            return Err(FramesResourceError);
        }
        Ok(resource)
    }
}
impl ResourceAddress for FramesResource {
    type Error = FramesResourceError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        match self {
            Self::Worlds(uri) => uri.to_uri().map_err(|_| FramesResourceError),
            Self::World(uri) => uri.to_uri().map_err(|_| FramesResourceError),
            Self::Revision(uri) => uri.to_uri().map_err(|_| FramesResourceError),
            Self::Frame(uri) => uri.to_uri().map_err(|_| FramesResourceError),
            Self::Operation(uri) => uri.to_uri().map_err(|_| FramesResourceError),
            Self::Usage(uri) => uri.to_uri().map_err(|_| FramesResourceError),
            Self::TaskUsage(uri) => uri.to_uri().map_err(|_| FramesResourceError),
            Self::Artifact(id) => ArtifactUri::presented(&uris::SCHEME, *id)
                .to_uri()
                .map_err(|_| FramesResourceError),
            Self::Document(doc) => ResourceUriBuilder::new(uris::DOCS_URI)
                .map_err(|_| FramesResourceError)?
                .segment(UriSegment::new(doc.as_str()).map_err(|_| FramesResourceError)?)
                .build()
                .map_err(|_| FramesResourceError),
            Self::Docs => ResourceUri::new(uris::DOCS_URI).map_err(|_| FramesResourceError),
            Self::Contract => ResourceUri::new(uris::CONTRACT_URI).map_err(|_| FramesResourceError),
            Self::WorkspaceApp => {
                ResourceUri::new(uris::WORKSPACE_APP_URI).map_err(|_| FramesResourceError)
            }
        }
    }
}
impl TryFrom<String> for FramesResource {
    type Error = FramesResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<FramesResource> for String {
    fn from(value: FramesResource) -> Self {
        value.to_uri().expect("admitted Frames address").to_string()
    }
}
