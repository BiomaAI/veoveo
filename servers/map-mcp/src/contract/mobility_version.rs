//! Immutable profile versions fit the Store's signed integer representation.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct MobilityProfileVersion(u64);

impl MobilityProfileVersion {
    pub const FIRST: Self = Self(1);
    pub fn new(value: u64) -> Result<Self, super::MobilityProfileError> {
        if value == 0 || value > i64::MAX as u64 {
            return Err(super::MobilityProfileError::InvalidVersion);
        }
        Ok(Self(value))
    }
    pub fn get(self) -> u64 {
        self.0
    }
}
impl TryFrom<u64> for MobilityProfileVersion {
    type Error = super::MobilityProfileError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl TryFrom<i64> for MobilityProfileVersion {
    type Error = super::MobilityProfileError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::new(
            value
                .try_into()
                .map_err(|_| super::MobilityProfileError::InvalidVersion)?,
        )
    }
}
impl From<MobilityProfileVersion> for u64 {
    fn from(value: MobilityProfileVersion) -> Self {
        value.0
    }
}
impl fmt::Display for MobilityProfileVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for MobilityProfileVersion {
    type Err = super::MobilityProfileError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let version = Self::new(
            value
                .parse()
                .map_err(|_| super::MobilityProfileError::InvalidVersion)?,
        )?;
        if version.to_string() != value {
            return Err(super::MobilityProfileError::InvalidVersion);
        }
        Ok(version)
    }
}

impl JsonSchema for MobilityProfileVersion {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MobilityProfileVersion".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"integer", "minimum":1, "maximum":i64::MAX})
    }
}
