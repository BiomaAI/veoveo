use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::TimeResourceError;

/// A stored calendar/epoch version in the positive signed database range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct TimeVersion(u64);

impl JsonSchema for TimeVersion {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TimeVersion".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"integer", "minimum":1, "maximum":i64::MAX})
    }
}

impl TimeVersion {
    pub fn new(value: u64) -> Result<Self, TimeResourceError> {
        if value == 0 || value > i64::MAX as u64 {
            return Err(TimeResourceError::InvalidVersion);
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for TimeVersion {
    type Error = TimeResourceError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TimeVersion> for u64 {
    fn from(value: TimeVersion) -> Self {
        value.0
    }
}

/// A relative TZDB key. The active authority establishes whether the zone exists.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct TimeZoneId(String);

impl TimeZoneId {
    pub fn new(value: impl Into<String>) -> Result<Self, TimeResourceError> {
        let value = value.into();
        if value.len() > 1024
            || value.split('/').any(|part| {
                part.is_empty()
                    || matches!(part, "." | "..")
                    || !part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+' | b'.')
                    })
            })
        {
            return Err(TimeResourceError::InvalidZone);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

impl TryFrom<String> for TimeZoneId {
    type Error = TimeResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TimeZoneId> for String {
    fn from(value: TimeZoneId) -> Self {
        value.0
    }
}

/// The documents embedded by this server's hosted surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeDocument {
    Agents,
    Design,
}

impl TimeDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }

    pub fn parse(value: &str) -> Result<Self, TimeResourceError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(TimeResourceError::UnknownResource),
        }
    }
}
