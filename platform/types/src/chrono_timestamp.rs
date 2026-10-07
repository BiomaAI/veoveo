//! Schema-only lexical carrier for Chrono UTC JSON timestamps.
//! Runtime fields keep DateTime<Utc>; their owner admits calendars and relationships.
use schemars::{JsonSchema, Schema, SchemaGenerator};

/// Inline schema carrier for `#[schemars(with = "ChronoUtcTimestampSchema")]`.
/// Use `Option<ChronoUtcTimestampSchema>` for an optional runtime timestamp.
/// This type supplies no runtime construction, storage or serialization behavior.
pub struct ChronoUtcTimestampSchema;

impl JsonSchema for ChronoUtcTimestampSchema {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ChronoUtcTimestamp".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": r"^([+-][0-9]{4,6}|[0-9]{4})-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])[Tt]([01][0-9]|2[0-3]):[0-5][0-9]:([0-5][0-9]|60)(\.[0-9]+)?([Zz]|[+-]([01][0-9]|2[0-3]):[0-5][0-9])$",
            "description": "RFC3339 timestamps with Chrono signed extended years and leap-second nanoseconds; receiving calendar and chronology admission is required."
        })
    }
}
