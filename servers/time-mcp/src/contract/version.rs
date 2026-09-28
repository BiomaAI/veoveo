use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeVersionError {
    InvalidVersion,
    InvalidGuard,
    Exhausted,
}

impl fmt::Display for TimeVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidVersion => "Time version must be in 1..=9223372036854775807",
            Self::InvalidGuard => "Time version guard must be in 0..=9223372036854775807",
            Self::Exhausted => "Time record_version is exhausted",
        })
    }
}

impl Error for TimeVersionError {}

/// A positive Time version that fits the database's signed integer range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct TimeVersion(u64);

impl TimeVersion {
    pub const FIRST: Self = Self(1);

    pub fn new(value: u64) -> Result<Self, TimeVersionError> {
        if value == 0 || value > i64::MAX as u64 {
            return Err(TimeVersionError::InvalidVersion);
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn checked_next(self) -> Result<Self, TimeVersionError> {
        self.0
            .checked_add(1)
            .filter(|next| *next <= i64::MAX as u64)
            .map(Self)
            .ok_or(TimeVersionError::Exhausted)
    }
}

impl TryFrom<u64> for TimeVersion {
    type Error = TimeVersionError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TimeVersion> for u64 {
    fn from(value: TimeVersion) -> Self {
        value.0
    }
}

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

/// Optimistic admission for an optional row. Zero on the wire means absence.
/// ```compile_fail
/// use veoveo_time_mcp::TimeWriteGuard;
/// let guard = TimeWriteGuard::Existing(1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub enum TimeWriteGuard {
    Absent,
    Existing(TimeVersion),
}

impl TimeWriteGuard {
    pub fn new(value: u64) -> Result<Self, TimeVersionError> {
        match value {
            0 => Ok(Self::Absent),
            value => TimeVersion::new(value)
                .map(Self::Existing)
                .map_err(|_| TimeVersionError::InvalidGuard),
        }
    }

    pub fn expected_version(self) -> u64 {
        match self {
            Self::Absent => 0,
            Self::Existing(version) => version.get(),
        }
    }

    pub fn next_version(self) -> Result<TimeVersion, TimeVersionError> {
        match self {
            Self::Absent => Ok(TimeVersion::FIRST),
            Self::Existing(version) => version.checked_next(),
        }
    }
}

impl TryFrom<u64> for TimeWriteGuard {
    type Error = TimeVersionError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TimeWriteGuard> for u64 {
    fn from(value: TimeWriteGuard) -> Self {
        value.expected_version()
    }
}

impl JsonSchema for TimeWriteGuard {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TimeWriteGuard".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"integer", "minimum":0, "maximum":i64::MAX})
    }
}
