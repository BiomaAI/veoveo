//! Stream-owned identifiers stay distinct through catalog and execution APIs.
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use veoveo_types::TaskId;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum StreamContractError {
    InvalidId(&'static str),
    InvalidResource,
    InvalidCursor,
    InvalidRelationship(&'static str),
}
impl fmt::Display for StreamContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(kind) => write!(f, "invalid Stream {kind} identifier"),
            Self::InvalidResource => f.write_str("invalid Stream resource address"),
            Self::InvalidCursor => f.write_str("invalid Stream collection cursor"),
            Self::InvalidRelationship(field) => write!(f, "inconsistent Stream {field}"),
        }
    }
}
impl std::error::Error for StreamContractError {}

// Preserve the published scalar schema while domain admission checks its profile.
macro_rules! string_schema {
    ($name:ident) => {
        impl schemars::JsonSchema for $name {
            fn inline_schema() -> bool {
                true
            }
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
                <String as schemars::JsonSchema>::json_schema(generator)
            }
        }
    };
}
pub(super) use string_schema;

#[derive(
    veoveo_types::Id, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=StreamContractError,validate=|value| validate_catalog_id(value,"pipeline"),schema=String::json_schema,schema_inline)]
pub struct PipelineId(String);

#[derive(
    veoveo_types::Id, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(string,constructor=parse,error=StreamContractError,validate=|value| validate_catalog_id(value,"model"),schema=String::json_schema,schema_inline)]
pub struct ModelId(String);

/// Identity of a Stream run backed by a native UUIDv7 Task.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RunId(TaskId);
impl RunId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let value = value.as_ref();
        let task: TaskId = value
            .parse()
            .map_err(|_| StreamContractError::InvalidId("run"))?;
        let id = Self::try_from(task)?;
        if id.to_string() != value {
            return Err(StreamContractError::InvalidId("run"));
        }
        Ok(id)
    }
    pub fn task_id(self) -> TaskId {
        self.0
    }
}
impl TryFrom<TaskId> for RunId {
    type Error = StreamContractError;
    fn try_from(value: TaskId) -> Result<Self, Self::Error> {
        if value.as_uuid().get_version_num() != 7
            || value.as_uuid().get_variant() != uuid::Variant::RFC4122
        {
            return Err(StreamContractError::InvalidId("run"));
        }
        Ok(Self(value))
    }
}
impl TryFrom<String> for RunId {
    type Error = StreamContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl FromStr for RunId {
    type Err = StreamContractError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl From<RunId> for String {
    fn from(value: RunId) -> Self {
        value.to_string()
    }
}
impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
string_schema!(RunId);

/// Process-local live-session identity, distinct from a recording run's Task.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionId(uuid::Uuid);
impl SessionId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, StreamContractError> {
        let value = value.as_ref();
        let uuid =
            uuid::Uuid::parse_str(value).map_err(|_| StreamContractError::InvalidId("session"))?;
        let id = Self::try_from(uuid)?;
        if id.to_string() != value {
            return Err(StreamContractError::InvalidId("session"));
        }
        Ok(id)
    }
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl TryFrom<uuid::Uuid> for SessionId {
    type Error = StreamContractError;
    fn try_from(value: uuid::Uuid) -> Result<Self, Self::Error> {
        if value.get_version_num() != 7 || value.get_variant() != uuid::Variant::RFC4122 {
            return Err(StreamContractError::InvalidId("session"));
        }
        Ok(Self(value))
    }
}
impl TryFrom<String> for SessionId {
    type Error = StreamContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl FromStr for SessionId {
    type Err = StreamContractError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl From<SessionId> for String {
    fn from(value: SessionId) -> Self {
        value.to_string()
    }
}
impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
string_schema!(SessionId);

fn validate_catalog_id(value: &str, kind: &'static str) -> Result<(), StreamContractError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        || !value.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(StreamContractError::InvalidId(kind));
    }
    Ok(())
}
