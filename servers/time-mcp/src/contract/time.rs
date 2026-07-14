use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{AuthorityReleaseId, MissionEpochId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Disambiguation {
    Reject,
    Earlier,
    Later,
}

impl Default for Disambiguation {
    fn default() -> Self {
        Self::Reject
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeScale {
    Utc,
    Tai,
    Tt,
    Tdb,
    Gpst,
    Gst,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthorityBinding {
    pub tzdb_release_id: AuthorityReleaseId,
    pub leap_seconds_release_id: AuthorityReleaseId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeInstant {
    /// Integral TAI seconds elapsed since 1970-01-01 00:00:00 TAI.
    pub tai_seconds_since_1970: i64,
    pub nanosecond: u32,
    pub uncertainty_nanoseconds: u64,
    pub authority: AuthorityBinding,
}

impl TimeInstant {
    pub fn total_nanoseconds(&self) -> i128 {
        i128::from(self.tai_seconds_since_1970) * 1_000_000_000 + i128::from(self.nanosecond)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CivilTime {
    pub local_datetime: String,
    pub zone_id: String,
    pub tzdb_release_id: AuthorityReleaseId,
    #[serde(default)]
    pub disambiguation: Disambiguation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeWindow {
    /// Inclusive lower bound.
    pub start: TimeInstant,
    /// Exclusive upper bound.
    pub end: TimeInstant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum TimeExpression {
    Rfc3339 {
        value: String,
    },
    Rfc9557 {
        value: String,
        #[serde(default)]
        disambiguation: Disambiguation,
    },
    Civil {
        value: CivilTime,
    },
    Unix {
        seconds: i64,
        #[serde(default)]
        nanosecond: u32,
    },
    Tai {
        seconds_since_1970: i64,
        #[serde(default)]
        nanosecond: u32,
    },
    Gps {
        week: u32,
        seconds_of_week: f64,
    },
    JulianTai {
        day: f64,
    },
    MilitaryDtg {
        value: String,
    },
    EpochRelative {
        epoch_id: MissionEpochId,
        offset_nanoseconds: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResolveTimeRequest {
    pub expression: TimeExpression,
    #[serde(default)]
    pub additional_uncertainty_nanoseconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResolveTimeOutput {
    pub instant: TimeInstant,
    pub utc_rfc3339: String,
    pub military_dtg: String,
    pub unix_seconds: i64,
    pub gps_week: Option<u32>,
    pub gps_seconds_of_week: Option<f64>,
    pub julian_day_tai: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConvertTimeRequest {
    pub instant: TimeInstant,
    #[serde(default)]
    pub zone_ids: Vec<String>,
    #[serde(default)]
    pub scales: Vec<TimeScale>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ZonedRepresentation {
    pub zone_id: String,
    pub rfc9557: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScaleRepresentation {
    pub scale: TimeScale,
    pub seconds: f64,
    pub reference_epoch: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConvertTimeOutput {
    pub canonical: ResolveTimeOutput,
    pub zoned: Vec<ZonedRepresentation>,
    pub scales: Vec<ScaleRepresentation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClockQuality {
    pub synchronized: bool,
    pub estimated_offset_nanoseconds: i64,
    pub error_bound_nanoseconds: u64,
    pub stratum: u8,
    pub holdover_age_seconds: Option<u64>,
    pub source_diversity: u32,
    pub traceability: Vec<String>,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClockQualityPolicy {
    pub maximum_error_nanoseconds: u64,
    pub maximum_stratum: u8,
    pub minimum_source_diversity: u32,
    pub maximum_holdover_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClockAssessment {
    pub quality: ClockQuality,
    pub policy: ClockQualityPolicy,
    pub acceptable: bool,
    pub violations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssessClockRequest {
    pub policy: Option<ClockQualityPolicy>,
}
