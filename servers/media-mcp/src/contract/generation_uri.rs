//! Address of one completed generation's retained result.
use super::{MediaPredictionError, MediaPredictionId, MediaPredictionUri};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MediaGenerationUri {
    wire: ResourceUri,
    prediction: MediaPredictionId,
}

impl MediaGenerationUri {
    pub const TEMPLATE: &str = "media://prediction/{id}/result";

    /// ```compile_fail
    /// use veoveo_media_mcp::contract::MediaGenerationUri;
    /// use veoveo_types::TaskId;
    /// MediaGenerationUri::new(TaskId::new());
    /// ```
    pub fn new(prediction: MediaPredictionId) -> Self {
        let wire = ResourceUriBuilder::new("media://prediction")
            .expect("declared Media prediction root")
            .segment(UriSegment::new(prediction.as_str()).expect("checked prediction ID"))
            .segment(UriSegment::new("result").expect("declared result segment"))
            .build()
            .expect("typed generation address");
        Self { wire, prediction }
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, MediaPredictionError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| MediaPredictionError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        if parts.scheme() != "media"
            || parts.authority() != "prediction"
            || parts.has_query()
            || path.len() != 2
            || path[1] != "result"
        {
            return Err(MediaPredictionError);
        }
        let uri = Self::new(MediaPredictionId::new(path[0].as_ref())?);
        if uri.as_str() != value {
            return Err(MediaPredictionError);
        }
        Ok(uri)
    }

    pub fn prediction_id(&self) -> &MediaPredictionId {
        &self.prediction
    }
    pub fn prediction_uri(&self) -> MediaPredictionUri {
        MediaPredictionUri::new(self.prediction.clone())
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl ResourceAddress for MediaGenerationUri {
    type Error = MediaPredictionError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.wire.clone())
    }
}
impl TryFrom<String> for MediaGenerationUri {
    type Error = MediaPredictionError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MediaGenerationUri> for String {
    fn from(value: MediaGenerationUri) -> Self {
        value.wire.into()
    }
}
impl fmt::Display for MediaGenerationUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
