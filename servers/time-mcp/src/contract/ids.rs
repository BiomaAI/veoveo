use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = String, validate = |value| validate_public_id(value, "time-source-"))]
pub struct TimeSourceId(String);
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = String, validate = |value| validate_public_id(value, "time-release-"))]
pub struct AuthorityReleaseId(String);
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = String, validate = |value| validate_public_id(value, "time-acquisition-"))]
pub struct TimeAcquisitionId(String);
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = String, validate = |value| validate_public_id(value, "calendar-"))]
pub struct CalendarId(String);
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = String, validate = |value| validate_public_id(value, "epoch-"))]
pub struct MissionEpochId(String);
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = String, validate = |value| validate_public_id(value, "event-"))]
pub struct TemporalEventId(String);

fn validate_public_id(value: &str, prefix: &'static str) -> Result<(), String> {
    if value.len() < prefix.len() + 1
        || value.len() > 128
        || !value.starts_with(prefix)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(format!("expected {} identifier", prefix));
    }
    Ok(())
}
