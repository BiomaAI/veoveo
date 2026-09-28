//! Provider prediction identity and its public resource address.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaPredictionError;
impl fmt::Display for MediaPredictionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a bounded Media prediction identity or canonical prediction address")
    }
}
impl std::error::Error for MediaPredictionError {}

/// Opaque provider identity, distinct from native Task and Store job identities.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct MediaPredictionId(String);
impl MediaPredictionId {
    pub fn new(value: impl Into<String>) -> Result<Self, MediaPredictionError> {
        let value = value.into();
        if value.len() > 512 || UriSegment::new(value.clone()).is_err() {
            return Err(MediaPredictionError);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for MediaPredictionId {
    type Error = MediaPredictionError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl FromStr for MediaPredictionId {
    type Err = MediaPredictionError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
impl From<MediaPredictionId> for String {
    fn from(value: MediaPredictionId) -> Self {
        value.0
    }
}
impl fmt::Display for MediaPredictionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MediaPredictionUri {
    wire: ResourceUri,
    id: MediaPredictionId,
}
impl MediaPredictionUri {
    pub const TEMPLATE: &str = "media://prediction/{id}";
    /// ```compile_fail
    /// use veoveo_media_mcp::contract::MediaPredictionUri;
    /// use veoveo_types::TaskId;
    /// MediaPredictionUri::new(TaskId::new());
    /// ```
    pub fn new(id: MediaPredictionId) -> Self {
        let wire = ResourceUriBuilder::new("media://prediction")
            .expect("declared prediction root")
            .segment(UriSegment::new(id.as_str()).expect("checked prediction identity"))
            .build()
            .expect("typed prediction address");
        Self { wire, id }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MediaPredictionError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MediaPredictionError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        if parts.scheme() != "media"
            || parts.authority() != "prediction"
            || parts.has_query()
            || path.len() != 1
        {
            return Err(MediaPredictionError);
        }
        let uri = Self::new(MediaPredictionId::new(path[0].as_ref())?);
        if uri.as_str() != value {
            return Err(MediaPredictionError);
        }
        Ok(uri)
    }
    pub fn id(&self) -> &MediaPredictionId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl ResourceAddress for MediaPredictionUri {
    type Error = MediaPredictionError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.wire.clone())
    }
}
impl TryFrom<String> for MediaPredictionUri {
    type Error = MediaPredictionError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MediaPredictionUri> for String {
    fn from(value: MediaPredictionUri) -> Self {
        value.wire.into()
    }
}
impl fmt::Display for MediaPredictionUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
