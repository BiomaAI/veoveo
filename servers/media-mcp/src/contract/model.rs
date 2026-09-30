//! Provider model names admitted as bounded slash-separated route components.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MediaModelId(String);
impl MediaModelId {
    pub fn parse(value: impl Into<String>) -> Result<Self, MediaModelUriError> {
        let value = value.into();
        if value.len() > 512
            || value.split('/').any(|part| {
                part.is_empty()
                    || matches!(part, "." | "..")
                    || !part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
            })
        {
            return Err(MediaModelUriError);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}
impl fmt::Display for MediaModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for MediaModelId {
    type Err = MediaModelUriError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl TryFrom<String> for MediaModelId {
    type Error = MediaModelUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MediaModelId> for String {
    fn from(value: MediaModelId) -> Self {
        value.0
    }
}

/// ```compile_fail
/// use veoveo_media_mcp::contract::MediaModelUri;
/// MediaModelUri::new("openai/gpt-image-2/edit");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MediaModelUri {
    wire: ResourceUri,
    model: MediaModelId,
}
impl MediaModelUri {
    pub const TEMPLATE: &str = "media://model/{+model_id}";
    pub fn new(model: MediaModelId) -> Self {
        let mut builder = ResourceUriBuilder::new("media://model").expect("declared model root");
        for part in model.components() {
            builder = builder.segment(UriSegment::new(part).expect("admitted model component"));
        }
        Self {
            wire: builder.build().expect("typed model address"),
            model,
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, MediaModelUriError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MediaModelUriError)?;
        if parts.scheme() != "media" || parts.authority() != "model" || parts.has_query() {
            return Err(MediaModelUriError);
        }
        let segments = parts.path_segments().collect::<Vec<_>>();
        let model = MediaModelId::parse(
            segments
                .iter()
                .map(|part| part.as_ref())
                .collect::<Vec<_>>()
                .join("/"),
        )?;
        let address = Self::new(model);
        if address.as_str() != value {
            return Err(MediaModelUriError);
        }
        Ok(address)
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
    pub fn model_id(&self) -> &MediaModelId {
        &self.model
    }
}
impl ResourceAddress for MediaModelUri {
    type Error = MediaModelUriError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.wire.clone())
    }
}
impl fmt::Display for MediaModelUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.wire.fmt(f)
    }
}
impl TryFrom<String> for MediaModelUri {
    type Error = MediaModelUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MediaModelUri> for String {
    fn from(value: MediaModelUri) -> Self {
        value.wire.to_string()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaModelUriError;
impl fmt::Display for MediaModelUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Media model identity or URI")
    }
}
impl std::error::Error for MediaModelUriError {}
