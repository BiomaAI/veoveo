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
pub struct RecurrenceRule {
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
pub struct CalendarWindow {
    pub start_local: String,
    pub end_local: String,
    pub recurrence: RecurrenceRule,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OperationalCalendar {
    pub calendar_id: CalendarId,
    pub version: u64,
    pub name: String,
    pub zone_id: String,
    pub windows: Vec<CalendarWindow>,
    #[serde(default)]
    pub excluded_dates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MissionEpoch {
    pub epoch_id: MissionEpochId,
    pub name: String,
    pub instant: TimeInstant,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
pub struct ScheduleOccurrence {
    pub sequence: u32,
    pub window: TimeWindow,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
pub struct EvaluateWindowsRequest {
    pub operation: WindowOperation,
    pub left: Vec<TimeWindow>,
    #[serde(default)]
    pub right: Vec<TimeWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EvaluateWindowsOutput {
    pub windows: Vec<TimeWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimelinePoint {
    pub name: String,
    pub at: TimeExpression,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimelineConstraint {
    pub predecessor: String,
    pub successor: String,
    #[serde(default)]
    pub minimum_separation_nanoseconds: u64,
    pub maximum_separation_nanoseconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateTimelineRequest {
    pub points: Vec<TimelinePoint>,
    pub constraints: Vec<TimelineConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimelineViolation {
    pub constraint_index: u32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateTimelineOutput {
    pub valid: bool,
    pub violations: Vec<TimelineViolation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalEvent {
    pub event_id: TemporalEventId,
    pub name: String,
    pub due: TimeInstant,
    pub state: TemporalEventState,
    pub record_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemporalEventState {
    Scheduled,
    Due,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateTemporalEventRequest {
    pub name: String,
    pub due: TimeInstant,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CancelTemporalEventRequest {
    pub event_id: TemporalEventId,
    pub expected_record_version: u64,
}
