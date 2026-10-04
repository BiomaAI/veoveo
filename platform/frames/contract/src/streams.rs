//! Frames consumes stream references without owning the producer's route vocabulary.
use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceUri, ResourceUriError, ResourceUriParts};

/// A concrete producer resource referenced by a dynamic frame transform.
///
/// Producers construct their address with their own typed resource builder, then
/// convert its `ResourceUri` through `TryFrom`. Parsing validates URI components;
/// the producer still owns route meaning, supported queries, and authorization.
///
/// ```compile_fail
/// use veoveo_frames_contract::{FrameEntityPath, FrameParentTransform};
/// let transform = FrameParentTransform::DynamicStream {
///     stream_uri: "producer://session/run".to_owned(),
///     entity_path: FrameEntityPath::new("vehicle/body").unwrap(),
/// };
/// ```
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct FrameStreamUri(ResourceUri);

impl FrameStreamUri {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ResourceUriError> {
        ResourceUriParts::parse(value.as_ref()).map(Self::from)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn as_resource_uri(&self) -> &ResourceUri {
        &self.0
    }
}

impl From<ResourceUriParts> for FrameStreamUri {
    fn from(value: ResourceUriParts) -> Self {
        Self(value.into_uri())
    }
}

impl TryFrom<ResourceUri> for FrameStreamUri {
    type Error = ResourceUriError;

    fn try_from(value: ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
}

impl TryFrom<String> for FrameStreamUri {
    type Error = ResourceUriError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl FromStr for FrameStreamUri {
    type Err = ResourceUriError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl From<FrameStreamUri> for ResourceUri {
    fn from(value: FrameStreamUri) -> Self {
        value.0
    }
}

impl From<FrameStreamUri> for String {
    fn from(value: FrameStreamUri) -> Self {
        value.0.into()
    }
}

impl fmt::Display for FrameStreamUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An entity selector carried unchanged to the producer of a frame transform.
/// It is not a filesystem path. The producer owns its selector grammar.
///
/// ```compile_fail
/// use veoveo_frames_contract::{FrameParentTransform, FrameStreamUri};
/// let transform = FrameParentTransform::DynamicStream {
///     stream_uri: FrameStreamUri::parse("producer://session/run").unwrap(),
///     entity_path: "vehicle/body".to_owned(),
/// };
/// ```
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct FrameEntityPath(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "frame entity path must be nonblank, at most 2048 bytes, and contain no control characters"
)]
pub struct FrameEntityPathError;

impl FrameEntityPath {
    pub fn new(value: impl Into<String>) -> Result<Self, FrameEntityPathError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 2_048 || value.chars().any(char::is_control) {
            return Err(FrameEntityPathError);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for FrameEntityPath {
    type Error = FrameEntityPathError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl FromStr for FrameEntityPath {
    type Err = FrameEntityPathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl From<FrameEntityPath> for String {
    fn from(value: FrameEntityPath) -> Self {
        value.0
    }
}

impl fmt::Display for FrameEntityPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
