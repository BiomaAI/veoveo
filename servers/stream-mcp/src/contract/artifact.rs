//! Stream owns its presentation of shared Artifact identities.
use std::{fmt, str::FromStr, sync::LazyLock};

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::{ArtifactAddress, ArtifactId, ArtifactUri};
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceUri};

static SCHEME: LazyLock<ResourceScheme> =
    LazyLock::new(|| ResourceScheme::parse("stream").expect("declared Stream scheme"));

/// A Stream presentation built from the Artifact owner's occurrence type.
/// ```compile_fail
/// use veoveo_stream_mcp::contract::StreamArtifactUri;
/// StreamArtifactUri::new("01983da0-0000-7000-8000-000000000001");
/// ```
/// ```compile_fail
/// use veoveo_stream_mcp::contract::StreamArtifactUri;
/// StreamArtifactUri::new(veoveo_types::TaskId::new());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StreamArtifactUri(ArtifactUri);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("expected a Stream Artifact URI with a UUIDv7 occurrence")]
pub struct StreamArtifactUriError;

impl StreamArtifactUri {
    pub fn new(id: ArtifactId) -> Self {
        Self(ArtifactUri::presented(&SCHEME, id))
    }
    pub fn parse(value: &str) -> Result<Self, StreamArtifactUriError> {
        let uri = ArtifactUri::parse(value).map_err(|_| StreamArtifactUriError)?;
        if !matches!(uri.address(), ArtifactAddress::Presented { scheme, .. } if scheme == &*SCHEME)
        {
            return Err(StreamArtifactUriError);
        }
        Ok(Self(uri))
    }
    pub fn artifact_id(&self) -> ArtifactId {
        self.0.artifact_id()
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    pub fn to_uri(&self) -> ResourceUri {
        self.0.as_resource_uri().clone()
    }
}
impl fmt::Display for StreamArtifactUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for StreamArtifactUri {
    type Err = StreamArtifactUriError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl TryFrom<String> for StreamArtifactUri {
    type Error = StreamArtifactUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<StreamArtifactUri> for String {
    fn from(value: StreamArtifactUri) -> Self {
        value.to_string()
    }
}
impl ResourceAddress for StreamArtifactUri {
    type Error = StreamArtifactUriError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.to_uri())
    }
}
impl JsonSchema for StreamArtifactUri {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "StreamArtifactUri".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
    fn inline_schema() -> bool {
        true
    }
}
