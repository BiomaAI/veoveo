use super::{ClockQualityPolicy, SubsecondNanoseconds, TimeInstant};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{AuthorityReleaseId, MissionEpochId, ResolveTimeOutput};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Disambiguation {
    #[default]
    Reject,
    Earlier,
    Later,
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
#[serde(deny_unknown_fields)]
#[schemars(rename = "CivilTime")]
pub struct CivilTimeValue {
    pub local_datetime: String,
    pub zone_id: String,
    pub tzdb_release_id: AuthorityReleaseId,
    #[serde(default)]
    pub disambiguation: Disambiguation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "format", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
#[schemars(rename = "TimeExpression")]
pub enum TimeExpressionValue {
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
        nanosecond: SubsecondNanoseconds,
    },
    Tai {
        seconds_since_1970: i64,
        #[serde(default)]
        nanosecond: SubsecondNanoseconds,
    },
    Gps {
        week: u32,
        seconds_of_week: super::admission::GpsSecondsOfWeek,
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
#[serde(deny_unknown_fields)]
pub struct ResolveTimeRequest {
    pub expression: TimeExpression,
    #[serde(default)]
    pub additional_uncertainty_nanoseconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[schemars(rename = "ScaleRepresentation")]
pub struct ScaleRepresentationValue {
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
#[schemars(rename = "ClockQuality")]
pub struct ClockQualityValue {
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
#[schemars(rename = "ClockAssessment")]
pub struct ClockAssessmentValue {
    pub quality: ClockQuality,
    pub policy: ClockQualityPolicy,
    pub acceptable: bool,
    pub violations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssessClockRequest {
    pub policy: Option<ClockQualityPolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CivilTimeValue", into = "CivilTimeValue")]
pub struct CivilTime(veoveo_types::Checked<CivilTimeValue>);
impl std::ops::Deref for CivilTime {
    type Target = CivilTimeValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for CivilTime {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CivilTime".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        CivilTimeValue::json_schema(generator)
    }
}
impl TryFrom<CivilTimeValue> for CivilTime {
    type Error = super::admission::TimeValueError;
    fn try_from(value: CivilTimeValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<CivilTime> for CivilTimeValue {
    fn from(value: CivilTime) -> Self {
        value.0.into_inner()
    }
}
impl CivilTimeValue {
    pub fn build(self) -> Result<CivilTime, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for CivilTimeValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        super::admission::zone(&self.zone_id)?;
        jiff::civil::DateTime::strptime("%Y-%m-%dT%H:%M:%S", &self.local_datetime)
            .or_else(|_| self.local_datetime.parse::<jiff::civil::DateTime>())
            .map_err(|_| super::admission::TimeValueError("invalid civil datetime"))?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "TimeExpressionValue", into = "TimeExpressionValue")]
pub struct TimeExpression(veoveo_types::Checked<TimeExpressionValue>);
impl std::ops::Deref for TimeExpression {
    type Target = TimeExpressionValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for TimeExpression {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TimeExpression".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        TimeExpressionValue::json_schema(generator)
    }
}
impl TryFrom<TimeExpressionValue> for TimeExpression {
    type Error = super::admission::TimeValueError;
    fn try_from(value: TimeExpressionValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<TimeExpression> for TimeExpressionValue {
    fn from(value: TimeExpression) -> Self {
        value.0.into_inner()
    }
}
impl TimeExpressionValue {
    pub fn build(self) -> Result<TimeExpression, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for TimeExpressionValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        match self {
            Self::Gps {
                seconds_of_week, ..
            } => super::admission::gps(seconds_of_week.get())?,
            Self::JulianTai { day } if !day.is_finite() => {
                return Err(super::admission::TimeValueError(
                    "Julian day must be finite",
                ));
            }
            Self::Rfc3339 { value } => {
                value
                    .parse::<jiff::Timestamp>()
                    .map_err(|_| super::admission::TimeValueError("invalid RFC 3339 timestamp"))?;
            }
            Self::Rfc9557 { value, .. } => {
                let pieces = jiff::fmt::temporal::DateTimeParser::new()
                    .parse_pieces(value)
                    .map_err(|_| super::admission::TimeValueError("invalid RFC 9557 timestamp"))?;
                if pieces.time_zone_annotation().is_none() || pieces.time().is_none() {
                    return Err(super::admission::TimeValueError(
                        "RFC 9557 timestamp requires a zone annotation",
                    ));
                }
            }
            Self::MilitaryDtg { value } => {
                super::admission::parse_military_dtg(value)
                    .map_err(|_| super::admission::TimeValueError("invalid military datetime"))?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "ScaleRepresentationValue",
    into = "ScaleRepresentationValue"
)]
pub struct ScaleRepresentation(veoveo_types::Checked<ScaleRepresentationValue>);
impl std::ops::Deref for ScaleRepresentation {
    type Target = ScaleRepresentationValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ScaleRepresentation {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ScaleRepresentation".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ScaleRepresentationValue::json_schema(generator)
    }
}
impl TryFrom<ScaleRepresentationValue> for ScaleRepresentation {
    type Error = super::admission::TimeValueError;
    fn try_from(value: ScaleRepresentationValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ScaleRepresentation> for ScaleRepresentationValue {
    fn from(value: ScaleRepresentation) -> Self {
        value.0.into_inner()
    }
}
impl ScaleRepresentationValue {
    pub fn build(self) -> Result<ScaleRepresentation, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ScaleRepresentationValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        if !self.seconds.is_finite() {
            return Err(super::admission::TimeValueError(
                "scale seconds must be finite",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ClockQualityValue", into = "ClockQualityValue")]
pub struct ClockQuality(veoveo_types::Checked<ClockQualityValue>);
impl std::ops::Deref for ClockQuality {
    type Target = ClockQualityValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ClockQuality {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ClockQuality".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ClockQualityValue::json_schema(generator)
    }
}
impl TryFrom<ClockQualityValue> for ClockQuality {
    type Error = super::admission::TimeValueError;
    fn try_from(value: ClockQualityValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ClockQuality> for ClockQualityValue {
    fn from(value: ClockQuality) -> Self {
        value.0.into_inner()
    }
}
impl ClockQualityValue {
    pub fn build(self) -> Result<ClockQuality, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ClockQualityValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        chrono::DateTime::parse_from_rfc3339(&self.observed_at)
            .map_err(|_| super::admission::TimeValueError("invalid clock observation timestamp"))?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ClockAssessmentValue", into = "ClockAssessmentValue")]
pub struct ClockAssessment(veoveo_types::Checked<ClockAssessmentValue>);
impl std::ops::Deref for ClockAssessment {
    type Target = ClockAssessmentValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ClockAssessment {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ClockAssessment".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ClockAssessmentValue::json_schema(generator)
    }
}
impl TryFrom<ClockAssessmentValue> for ClockAssessment {
    type Error = super::admission::TimeValueError;
    fn try_from(value: ClockAssessmentValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ClockAssessment> for ClockAssessmentValue {
    fn from(value: ClockAssessment) -> Self {
        value.0.into_inner()
    }
}
impl ClockAssessmentValue {
    pub fn build(self) -> Result<ClockAssessment, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ClockAssessmentValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.acceptable != self.violations.is_empty() {
            return Err(super::admission::TimeValueError(
                "clock assessment contradicts violations",
            ));
        }
        Ok(())
    }
}
