use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum ObservatoryScope {
    #[vocabulary(rename = "observatory:read")]
    Read,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ReadingId(String);

impl ReadingId {
    pub fn new(value: impl Into<String>) -> Result<Self, ObservatoryAddressError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(ObservatoryAddressError);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ReadingId {
    type Error = ObservatoryAddressError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<ReadingId> for String {
    fn from(value: ReadingId) -> Self {
        value.0
    }
}
impl FromStr for ReadingId {
    type Err = ObservatoryAddressError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservatoryDocument {
    Agents,
    Design,
}

impl ObservatoryDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
}

/// ```compile_fail
/// use veoveo_modular_fixture_mcp::contract::ObservatoryResource;
/// ObservatoryResource::Reading("sensor-a".to_owned());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservatoryResource {
    Docs,
    Document(ObservatoryDocument),
    Contract,
    Readings,
    Reading(ReadingId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected an Observatory resource with a valid reading ID and no query or escaped alias")]
pub struct ObservatoryAddressError;

impl ResourceAddress for ObservatoryResource {
    type Error = ObservatoryAddressError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        let parts = ResourceUriParts::parse(uri.as_str()).map_err(|_| ObservatoryAddressError)?;
        if parts.scheme() != "observatory" || parts.has_query() {
            return Err(ObservatoryAddressError);
        }
        let decoded: Vec<_> = parts.path_segments().collect();
        let path: Vec<_> = decoded.iter().map(|value| value.as_ref()).collect();
        let resource = match (parts.authority(), path.as_slice()) {
            ("docs", []) => Self::Docs,
            ("docs", ["agents"]) => Self::Document(ObservatoryDocument::Agents),
            ("docs", ["design"]) => Self::Document(ObservatoryDocument::Design),
            ("contract", []) => Self::Contract,
            ("readings", []) => Self::Readings,
            ("reading", [id]) => Self::Reading(ReadingId::new(*id)?),
            _ => return Err(ObservatoryAddressError),
        };
        if resource.to_uri()? != *uri {
            return Err(ObservatoryAddressError);
        }
        Ok(resource)
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let (base, segment) = match self {
            Self::Docs => ("observatory://docs", None),
            Self::Document(id) => ("observatory://docs", Some(id.as_str())),
            Self::Contract => ("observatory://contract", None),
            Self::Readings => ("observatory://readings", None),
            Self::Reading(id) => ("observatory://reading", Some(id.as_str())),
        };
        let mut builder = ResourceUriBuilder::new(base).map_err(|_| ObservatoryAddressError)?;
        if let Some(segment) = segment {
            builder =
                builder.segment(UriSegment::new(segment).map_err(|_| ObservatoryAddressError)?);
        }
        builder.build().map_err(|_| ObservatoryAddressError)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reading {
    pub id: ReadingId,
    pub value: i32,
}
