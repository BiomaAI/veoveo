//! Address profiles owned by tabular source admission.
use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::{ArtifactAddress, ArtifactId, ArtifactUri};
use veoveo_types::HttpsUrl;

/// At least one HTTPS source is required at construction and decoding.
/// ```compile_fail
/// use veoveo_duckdb_mcp::contract::DuckDbSourceUris;
/// DuckDbSourceUris::new("https://data.example.test/file.csv", []);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<HttpsUrl>", into = "Vec<HttpsUrl>")]
pub struct DuckDbSourceUris(Vec<HttpsUrl>);

impl DuckDbSourceUris {
    pub fn new(first: HttpsUrl, rest: impl IntoIterator<Item = HttpsUrl>) -> Self {
        Self(std::iter::once(first).chain(rest).collect())
    }
    pub fn as_slice(&self) -> &[HttpsUrl] {
        &self.0
    }
    pub fn iter(&self) -> std::slice::Iter<'_, HttpsUrl> {
        self.0.iter()
    }
}
impl TryFrom<Vec<HttpsUrl>> for DuckDbSourceUris {
    type Error = DuckDbSourceAddressError;
    fn try_from(value: Vec<HttpsUrl>) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(DuckDbSourceAddressError);
        }
        Ok(Self(value))
    }
}
impl From<DuckDbSourceUris> for Vec<HttpsUrl> {
    fn from(value: DuckDbSourceUris) -> Self {
        value.0
    }
}
impl JsonSchema for DuckDbSourceUris {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DuckDbSourceUris".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = Vec::<HttpsUrl>::json_schema(generator);
        schema.insert("minItems".into(), 1.into());
        schema
    }
}

/// A cross-server source uses the Artifact plane's neutral identity.
/// ```compile_fail
/// use veoveo_duckdb_mcp::contract::DuckDbArtifactSourceUri;
/// use veoveo_types::TaskId;
/// DuckDbArtifactSourceUri::new(TaskId::new());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbArtifactSourceUri(ArtifactUri);
impl DuckDbArtifactSourceUri {
    pub fn new(id: ArtifactId) -> Self {
        Self(ArtifactUri::plane(id))
    }
    pub fn parse(value: &str) -> Result<Self, DuckDbSourceAddressError> {
        let uri = ArtifactUri::parse(value).map_err(|_| DuckDbSourceAddressError)?;
        if !matches!(uri.address(), ArtifactAddress::Plane(_)) {
            return Err(DuckDbSourceAddressError);
        }
        Ok(Self(uri))
    }
    pub fn as_artifact_uri(&self) -> &ArtifactUri {
        &self.0
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
impl fmt::Display for DuckDbArtifactSourceUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for DuckDbArtifactSourceUri {
    type Err = DuckDbSourceAddressError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl TryFrom<String> for DuckDbArtifactSourceUri {
    type Error = DuckDbSourceAddressError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<DuckDbArtifactSourceUri> for String {
    fn from(value: DuckDbArtifactSourceUri) -> Self {
        value.to_string()
    }
}
impl JsonSchema for DuckDbArtifactSourceUri {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DuckDbArtifactSourceUri".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type":"string", "pattern":"^artifact://(?:[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|[0-9a-fA-F]{12}7[0-9a-fA-F]{3}[89aAbB][0-9a-fA-F]{15})$",
            "description":"Neutral Artifact plane URI naming a native UUIDv7 occurrence. Parsed by the Artifact contract."
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuckDbSourceAddressError;
impl fmt::Display for DuckDbSourceAddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a nonempty HTTPS source list or a neutral Artifact occurrence URI")
    }
}
impl std::error::Error for DuckDbSourceAddressError {}
