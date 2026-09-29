//! Recording identities and redacted admission errors.
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingContractError {
    Identity,
    Resource,
    Cursor,
}
impl fmt::Display for RecordingContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Identity => "recording ID must be a canonical RFC UUIDv7",
            Self::Resource => "invalid Recording resource address",
            Self::Cursor => "invalid Recording catalog cursor",
        })
    }
}
impl std::error::Error for RecordingContractError {}

macro_rules! string_schema {
    ($name:ident) => {
        impl schemars::JsonSchema for $name {
            fn inline_schema() -> bool {
                true
            }
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
                <String as schemars::JsonSchema>::json_schema(g)
            }
        }
    };
}
pub(super) use string_schema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecordingId(uuid::Uuid);
impl RecordingId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, RecordingContractError> {
        let value = value.as_ref();
        let uuid = uuid::Uuid::parse_str(value).map_err(|_| RecordingContractError::Identity)?;
        let id = Self::try_from(uuid)?;
        if id.to_string() != value {
            return Err(RecordingContractError::Identity);
        }
        Ok(id)
    }
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl Default for RecordingId {
    fn default() -> Self {
        Self::new()
    }
}
impl TryFrom<uuid::Uuid> for RecordingId {
    type Error = RecordingContractError;
    fn try_from(uuid: uuid::Uuid) -> Result<Self, Self::Error> {
        if uuid.get_version_num() != 7 || uuid.get_variant() != uuid::Variant::RFC4122 {
            return Err(RecordingContractError::Identity);
        }
        Ok(Self(uuid))
    }
}
impl TryFrom<String> for RecordingId {
    type Error = RecordingContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl FromStr for RecordingId {
    type Err = RecordingContractError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl From<RecordingId> for String {
    fn from(value: RecordingId) -> Self {
        value.to_string()
    }
}
impl fmt::Display for RecordingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
string_schema!(RecordingId);
