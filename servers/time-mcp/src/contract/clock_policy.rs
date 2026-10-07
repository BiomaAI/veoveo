use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockPolicyField {
    MaximumErrorNanoseconds,
    MaximumStratum,
    MinimumSourceDiversity,
    MaximumHoldoverSeconds,
}

impl ClockPolicyField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaximumErrorNanoseconds => "maximumErrorNanoseconds",
            Self::MaximumStratum => "maximumStratum",
            Self::MinimumSourceDiversity => "minimumSourceDiversity",
            Self::MaximumHoldoverSeconds => "maximumHoldoverSeconds",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockPolicyError {
    Missing(ClockPolicyField),
    OutOfRange(ClockPolicyField),
}

impl ClockPolicyError {
    pub fn field(self) -> ClockPolicyField {
        match self {
            Self::Missing(field) | Self::OutOfRange(field) => field,
        }
    }
}

impl fmt::Display for ClockPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(field) => write!(f, "clock policy requires {}", field.as_str()),
            Self::OutOfRange(field) => write!(
                f,
                "clock policy {} is outside its supported range",
                field.as_str()
            ),
        }
    }
}
impl Error for ClockPolicyError {}

/// Clock limits admitted through the builder or JSON decoding.
/// ```compile_fail
/// use veoveo_time_mcp::ClockQualityPolicy;
/// let policy = ClockQualityPolicy { maximum_error_nanoseconds: 1, maximum_stratum: 0,
///     minimum_source_diversity: 1, maximum_holdover_seconds: 1 };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "ClockQualityPolicyFields",
    into = "ClockQualityPolicyFields"
)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ClockQualityPolicy {
    maximum_error_nanoseconds: u64,
    maximum_stratum: u8,
    minimum_source_diversity: u32,
    maximum_holdover_seconds: u64,
}

impl ClockQualityPolicy {
    pub fn builder() -> ClockQualityPolicyBuilder {
        ClockQualityPolicyBuilder::default()
    }
    pub fn maximum_error_nanoseconds(&self) -> u64 {
        self.maximum_error_nanoseconds
    }
    pub fn maximum_stratum(&self) -> u8 {
        self.maximum_stratum
    }
    pub fn minimum_source_diversity(&self) -> u32 {
        self.minimum_source_diversity
    }
    pub fn maximum_holdover_seconds(&self) -> u64 {
        self.maximum_holdover_seconds
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClockQualityPolicyBuilder {
    maximum_error_nanoseconds: Option<u64>,
    maximum_stratum: Option<u8>,
    minimum_source_diversity: Option<u32>,
    maximum_holdover_seconds: Option<u64>,
}

impl ClockQualityPolicyBuilder {
    pub fn maximum_error_nanoseconds(mut self, value: u64) -> Self {
        self.maximum_error_nanoseconds = Some(value);
        self
    }
    pub fn maximum_stratum(mut self, value: u8) -> Self {
        self.maximum_stratum = Some(value);
        self
    }
    pub fn minimum_source_diversity(mut self, value: u32) -> Self {
        self.minimum_source_diversity = Some(value);
        self
    }
    pub fn maximum_holdover_seconds(mut self, value: u64) -> Self {
        self.maximum_holdover_seconds = Some(value);
        self
    }

    pub fn build(self) -> Result<ClockQualityPolicy, ClockPolicyError> {
        use ClockPolicyError::Missing;
        use ClockPolicyField::*;
        ClockQualityPolicyFields {
            maximum_error_nanoseconds: self
                .maximum_error_nanoseconds
                .ok_or(Missing(MaximumErrorNanoseconds))?,
            maximum_stratum: self.maximum_stratum.ok_or(Missing(MaximumStratum))?,
            minimum_source_diversity: self
                .minimum_source_diversity
                .ok_or(Missing(MinimumSourceDiversity))?,
            maximum_holdover_seconds: self
                .maximum_holdover_seconds
                .ok_or(Missing(MaximumHoldoverSeconds))?,
        }
        .try_into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct ClockQualityPolicyFields {
    #[schemars(range(min = 1, max = 9223372036854775807_u64))]
    maximum_error_nanoseconds: u64,
    #[schemars(range(min = 1, max = 15))]
    maximum_stratum: u8,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    minimum_source_diversity: u32,
    #[schemars(range(min = 1, max = 9223372036854775807_u64))]
    maximum_holdover_seconds: u64,
}

impl TryFrom<ClockQualityPolicyFields> for ClockQualityPolicy {
    type Error = ClockPolicyError;
    fn try_from(value: ClockQualityPolicyFields) -> Result<Self, Self::Error> {
        use ClockPolicyError::OutOfRange;
        use ClockPolicyField::*;
        if !(1..=i64::MAX as u64).contains(&value.maximum_error_nanoseconds) {
            return Err(OutOfRange(MaximumErrorNanoseconds));
        }
        if !(1..=15).contains(&value.maximum_stratum) {
            return Err(OutOfRange(MaximumStratum));
        }
        if value.minimum_source_diversity == 0 {
            return Err(OutOfRange(MinimumSourceDiversity));
        }
        if !(1..=i64::MAX as u64).contains(&value.maximum_holdover_seconds) {
            return Err(OutOfRange(MaximumHoldoverSeconds));
        }
        Ok(Self {
            maximum_error_nanoseconds: value.maximum_error_nanoseconds,
            maximum_stratum: value.maximum_stratum,
            minimum_source_diversity: value.minimum_source_diversity,
            maximum_holdover_seconds: value.maximum_holdover_seconds,
        })
    }
}

impl From<ClockQualityPolicy> for ClockQualityPolicyFields {
    fn from(value: ClockQualityPolicy) -> Self {
        Self {
            maximum_error_nanoseconds: value.maximum_error_nanoseconds,
            maximum_stratum: value.maximum_stratum,
            minimum_source_diversity: value.minimum_source_diversity,
            maximum_holdover_seconds: value.maximum_holdover_seconds,
        }
    }
}

impl JsonSchema for ClockQualityPolicy {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ClockQualityPolicy".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ClockQualityPolicyFields::json_schema(generator)
    }
}
