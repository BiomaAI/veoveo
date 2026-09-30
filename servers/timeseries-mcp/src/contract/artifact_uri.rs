//! The forecast handoff names one Artifact occurrence under this server's scheme.
use crate::uris;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_artifact_contract::{ArtifactAddress, ArtifactId, ArtifactUri};
use veoveo_types::{ResourceAddress, ResourceUri};

/// ```compile_fail
/// use veoveo_timeseries_mcp::contract::TimeseriesArtifactUri;
/// use veoveo_types::TaskId;
/// TimeseriesArtifactUri::new(TaskId::new());
/// ```
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct TimeseriesArtifactUri {
    wire: ResourceUri,
    artifact_id: ArtifactId,
}
impl TimeseriesArtifactUri {
    pub fn new(artifact_id: ArtifactId) -> Self {
        let uri = ArtifactUri::presented(&uris::SCHEME, artifact_id);
        Self {
            wire: uri.as_resource_uri().clone(),
            artifact_id,
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TimeseriesArtifactUriError> {
        let value = value.as_ref();
        let uri = ArtifactUri::parse(value).map_err(|_| TimeseriesArtifactUriError)?;
        if !matches!(uri.address(), ArtifactAddress::Presented { scheme, .. } if scheme == &*uris::SCHEME)
        {
            return Err(TimeseriesArtifactUriError);
        }
        let result = Self::new(uri.artifact_id());
        if result.as_str() != value {
            return Err(TimeseriesArtifactUriError);
        }
        Ok(result)
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
    pub fn artifact_id(&self) -> ArtifactId {
        self.artifact_id
    }
}
impl ResourceAddress for TimeseriesArtifactUri {
    type Error = TimeseriesArtifactUriError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.wire.clone())
    }
}
impl fmt::Display for TimeseriesArtifactUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.wire.fmt(formatter)
    }
}
impl TryFrom<String> for TimeseriesArtifactUri {
    type Error = TimeseriesArtifactUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<TimeseriesArtifactUri> for String {
    fn from(value: TimeseriesArtifactUri) -> Self {
        value.wire.to_string()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeseriesArtifactUriError;
impl fmt::Display for TimeseriesArtifactUriError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Timeseries artifact URI")
    }
}
impl std::error::Error for TimeseriesArtifactUriError {}
