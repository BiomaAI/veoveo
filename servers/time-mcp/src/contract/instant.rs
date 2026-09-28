use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::AuthorityBinding;

const NANOS_PER_SECOND: i128 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeCoordinateError {
    InvalidNanosecond,
    SecondsOverflow,
}

impl fmt::Display for TimeCoordinateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidNanosecond => "subsecond nanoseconds must be in 0..=999999999",
            Self::SecondsOverflow => "time coordinate exceeds the signed 64-bit seconds range",
        })
    }
}

impl Error for TimeCoordinateError {}

/// Nanoseconds within one integral second, distinct from a duration.
/// ```compile_fail
/// use veoveo_time_mcp::TimeInstant;
/// fn cannot_overflow_second(instant: &mut TimeInstant) {
///     instant.nanosecond = 1_000_000_000;
/// }
/// ```
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "u32", into = "u32")]
pub struct SubsecondNanoseconds(u32);

impl SubsecondNanoseconds {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(999_999_999);

    pub fn new(value: u32) -> Result<Self, TimeCoordinateError> {
        if value > Self::MAX.0 {
            return Err(TimeCoordinateError::InvalidNanosecond);
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for SubsecondNanoseconds {
    type Error = TimeCoordinateError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SubsecondNanoseconds> for u32 {
    fn from(value: SubsecondNanoseconds) -> Self {
        value.0
    }
}

impl JsonSchema for SubsecondNanoseconds {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SubsecondNanoseconds".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"integer", "minimum":0, "maximum":999_999_999})
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeInstant {
    /// Integral TAI seconds elapsed since 1970-01-01 00:00:00 TAI.
    pub tai_seconds_since_1970: i64,
    pub nanosecond: SubsecondNanoseconds,
    pub uncertainty_nanoseconds: u64,
    pub authority: AuthorityBinding,
}

impl TimeInstant {
    pub fn total_nanoseconds(&self) -> i128 {
        i128::from(self.tai_seconds_since_1970) * NANOS_PER_SECOND
            + i128::from(self.nanosecond.get())
    }

    /// Split a signed coordinate without wrapping seconds or losing its fraction.
    pub fn from_total_nanoseconds(
        total: i128,
        uncertainty_nanoseconds: u64,
        authority: AuthorityBinding,
    ) -> Result<Self, TimeCoordinateError> {
        let seconds = i64::try_from(total.div_euclid(NANOS_PER_SECOND))
            .map_err(|_| TimeCoordinateError::SecondsOverflow)?;
        let nanosecond = SubsecondNanoseconds::new(total.rem_euclid(NANOS_PER_SECOND) as u32)?;
        Ok(Self {
            tai_seconds_since_1970: seconds,
            nanosecond,
            uncertainty_nanoseconds,
            authority,
        })
    }
}
