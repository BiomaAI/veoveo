use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{CalendarId, MissionEpochId, TemporalEventId, TimeExpression, TimeInstant, TimeWindow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceFrequency {
    Daily,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "RecurrenceRule")]
#[serde(rename_all = "camelCase")]
pub struct RecurrenceRuleValue {
    pub frequency: RecurrenceFrequency,
    #[serde(default = "one")]
    pub interval: u32,
    #[serde(default)]
    pub weekdays: Vec<Weekday>,
    pub count: Option<u32>,
    pub until: Option<TimeInstant>,
}

const fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "CalendarWindow")]
#[serde(rename_all = "camelCase")]
pub struct CalendarWindowValue {
    pub start_local: String,
    pub end_local: String,
    pub recurrence: RecurrenceRule,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "OperationalCalendar")]
#[serde(rename_all = "camelCase")]
pub struct OperationalCalendarValue {
    pub calendar_id: CalendarId,
    pub version: super::TimeVersion,
    pub name: String,
    pub zone_id: String,
    pub windows: Vec<CalendarWindow>,
    #[serde(default)]
    pub excluded_dates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct MissionEpoch {
    pub epoch_id: MissionEpochId,
    pub name: String,
    pub instant: TimeInstant,
    pub version: super::TimeVersion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ExpandScheduleRequest {
    pub calendar: OperationalCalendar,
    pub horizon: TimeWindow,
    #[serde(default = "default_occurrence_limit")]
    pub maximum_occurrences: u32,
}

const fn default_occurrence_limit() -> u32 {
    10_000
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ScheduleOccurrence {
    pub sequence: u32,
    pub window: TimeWindow,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ExpandScheduleOutput {
    pub occurrences: Vec<ScheduleOccurrence>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WindowOperation {
    Union,
    Intersection,
    Difference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateWindowsRequest {
    pub operation: WindowOperation,
    pub left: Vec<TimeWindow>,
    #[serde(default)]
    pub right: Vec<TimeWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct EvaluateWindowsOutput {
    pub windows: Vec<TimeWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePoint {
    pub name: String,
    pub at: TimeExpression,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TimelineConstraint {
    pub predecessor: String,
    pub successor: String,
    #[serde(default)]
    pub minimum_separation_nanoseconds: u64,
    pub maximum_separation_nanoseconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "ValidateTimelineRequest")]
#[serde(rename_all = "camelCase")]
pub struct ValidateTimelineRequestValue {
    pub points: Vec<TimelinePoint>,
    pub constraints: Vec<TimelineConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimelineViolation {
    pub constraint_index: u32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "ValidateTimelineOutput")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ValidateTimelineOutputValue {
    pub valid: bool,
    pub violations: Vec<TimelineViolation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TemporalEvent {
    pub event_id: TemporalEventId,
    pub name: String,
    pub due: TimeInstant,
    pub state: TemporalEventState,
    pub record_version: super::TimeVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemporalEventState {
    Scheduled,
    Due,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CreateTemporalEventRequest {
    pub name: String,
    pub due: TimeInstant,
    pub idempotency_key: String,
}

/// Cancellation requires the version of an existing event.
/// ```compile_fail
/// use veoveo_time_mcp::{CancelTemporalEventRequest, TemporalEventId, TimeWriteGuard};
/// let request = CancelTemporalEventRequest {
///     event_id: TemporalEventId::parse("event-example").unwrap(),
///     expected_record_version: TimeWriteGuard::Absent,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CancelTemporalEventRequest {
    pub event_id: TemporalEventId,
    pub expected_record_version: super::TimeVersion,
}

/// One page of a Time collection, in the order declared by its resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CollectionPage<T, C> {
    pub items: Vec<T>,
    pub limit: usize,
    pub next_cursor: Option<C>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RecurrenceRuleValue", into = "RecurrenceRuleValue")]
pub struct RecurrenceRule(veoveo_types::Checked<RecurrenceRuleValue>);
impl std::ops::Deref for RecurrenceRule {
    type Target = RecurrenceRuleValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for RecurrenceRule {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RecurrenceRule".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        RecurrenceRuleValue::json_schema(generator)
    }
}
impl TryFrom<RecurrenceRuleValue> for RecurrenceRule {
    type Error = super::admission::TimeValueError;
    fn try_from(value: RecurrenceRuleValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<RecurrenceRule> for RecurrenceRuleValue {
    fn from(value: RecurrenceRule) -> Self {
        value.0.into_inner()
    }
}
impl RecurrenceRuleValue {
    pub fn build(self) -> Result<RecurrenceRule, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for RecurrenceRuleValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.interval == 0 || self.count == Some(0) {
            return Err(super::admission::TimeValueError(
                "recurrence interval and count must be positive",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CalendarWindowValue", into = "CalendarWindowValue")]
pub struct CalendarWindow(veoveo_types::Checked<CalendarWindowValue>);
impl std::ops::Deref for CalendarWindow {
    type Target = CalendarWindowValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for CalendarWindow {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CalendarWindow".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        CalendarWindowValue::json_schema(generator)
    }
}
impl TryFrom<CalendarWindowValue> for CalendarWindow {
    type Error = super::admission::TimeValueError;
    fn try_from(value: CalendarWindowValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<CalendarWindow> for CalendarWindowValue {
    fn from(value: CalendarWindow) -> Self {
        value.0.into_inner()
    }
}
impl CalendarWindowValue {
    pub fn build(self) -> Result<CalendarWindow, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for CalendarWindowValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        if super::admission::local(&self.end_local)? <= super::admission::local(&self.start_local)?
        {
            return Err(super::admission::TimeValueError(
                "calendar end must follow its start",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "OperationalCalendarValue",
    into = "OperationalCalendarValue"
)]
pub struct OperationalCalendar(veoveo_types::Checked<OperationalCalendarValue>);
impl std::ops::Deref for OperationalCalendar {
    type Target = OperationalCalendarValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for OperationalCalendar {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "OperationalCalendar".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        OperationalCalendarValue::json_schema(generator)
    }
}
impl TryFrom<OperationalCalendarValue> for OperationalCalendar {
    type Error = super::admission::TimeValueError;
    fn try_from(value: OperationalCalendarValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<OperationalCalendar> for OperationalCalendarValue {
    fn from(value: OperationalCalendar) -> Self {
        value.0.into_inner()
    }
}
impl OperationalCalendarValue {
    pub fn build(self) -> Result<OperationalCalendar, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for OperationalCalendarValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        super::admission::zone(&self.zone_id)?;
        for date in &self.excluded_dates {
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| {
                super::admission::TimeValueError("excluded date requires YYYY-MM-DD")
            })?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "ValidateTimelineRequestValue",
    into = "ValidateTimelineRequestValue"
)]
pub struct ValidateTimelineRequest(veoveo_types::Checked<ValidateTimelineRequestValue>);
impl std::ops::Deref for ValidateTimelineRequest {
    type Target = ValidateTimelineRequestValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ValidateTimelineRequest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ValidateTimelineRequest".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ValidateTimelineRequestValue::json_schema(generator)
    }
}
impl TryFrom<ValidateTimelineRequestValue> for ValidateTimelineRequest {
    type Error = super::admission::TimeValueError;
    fn try_from(value: ValidateTimelineRequestValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ValidateTimelineRequest> for ValidateTimelineRequestValue {
    fn from(value: ValidateTimelineRequest) -> Self {
        value.0.into_inner()
    }
}
impl ValidateTimelineRequestValue {
    pub fn build(self) -> Result<ValidateTimelineRequest, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ValidateTimelineRequestValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.points.len() > 100_000 || self.constraints.len() > 1_000_000 {
            return Err(super::admission::TimeValueError(
                "timeline exceeds point or constraint limit",
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for point in &self.points {
            if point.name.trim().is_empty() || !names.insert(&point.name) {
                return Err(super::admission::TimeValueError(
                    "timeline point names must be nonempty and unique",
                ));
            }
        }
        for constraint in &self.constraints {
            if !names.contains(&constraint.predecessor)
                || !names.contains(&constraint.successor)
                || constraint
                    .maximum_separation_nanoseconds
                    .is_some_and(|max| max < constraint.minimum_separation_nanoseconds)
            {
                return Err(super::admission::TimeValueError(
                    "invalid timeline constraint references or separation",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "ValidateTimelineOutputValue",
    into = "ValidateTimelineOutputValue"
)]
pub struct ValidateTimelineOutput(veoveo_types::Checked<ValidateTimelineOutputValue>);
impl std::ops::Deref for ValidateTimelineOutput {
    type Target = ValidateTimelineOutputValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ValidateTimelineOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ValidateTimelineOutput".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ValidateTimelineOutputValue::json_schema(generator)
    }
}
impl TryFrom<ValidateTimelineOutputValue> for ValidateTimelineOutput {
    type Error = super::admission::TimeValueError;
    fn try_from(value: ValidateTimelineOutputValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ValidateTimelineOutput> for ValidateTimelineOutputValue {
    fn from(value: ValidateTimelineOutput) -> Self {
        value.0.into_inner()
    }
}
impl ValidateTimelineOutputValue {
    pub fn build(self) -> Result<ValidateTimelineOutput, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ValidateTimelineOutputValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.valid != self.violations.is_empty() {
            return Err(super::admission::TimeValueError(
                "timeline outcome contradicts violations",
            ));
        }
        Ok(())
    }
}
