//! Public recorded-video values and validation without source access.

use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod source_snapshot;
pub use source_snapshot::{
    RecordingSourceError, RecordingSourceIdentity, RecordingSourceIdentityBuilder,
    RecordingSourceIdentityKind, RecordingSourceSnapshot, RecordingSourceSnapshotBuilder,
};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "RecordingVideoSelection")]
pub struct RecordingVideoSelectionBuilder {
    /// Canonical `recording://recordings/{recording_id}` URI.
    pub recording_uri: veoveo_recording_contract::RecordingUri,
    /// Exact Rerun entity path containing `VideoStream` samples.
    pub entity_path: String,
    /// Rerun duration, timestamp, or sequence timeline.
    pub timeline: String,
    pub range: IndexRange,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "IndexRange")]
pub struct IndexRangeBuilder {
    pub start: i64,
    pub end: i64,
}

impl veoveo_types::Check for IndexRangeBuilder {
    type Error = anyhow::Error;
    fn check(&self) -> Result<()> {
        ensure!(self.start <= self.end, "video range must be ordered");
        Ok(())
    }
}
impl IndexRangeBuilder {
    pub fn build(self) -> Result<IndexRange> {
        veoveo_types::Checked::new(self).map(IndexRange)
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct IndexRange(veoveo_types::Checked<IndexRangeBuilder>);
impl schemars::JsonSchema for IndexRange {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <IndexRangeBuilder as schemars::JsonSchema>::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        <IndexRangeBuilder as schemars::JsonSchema>::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <IndexRangeBuilder as schemars::JsonSchema>::json_schema(generator)
    }
}

impl std::ops::Deref for IndexRange {
    type Target = IndexRangeBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl IndexRange {
    pub fn new(start: i64, end: i64) -> Result<Self> {
        IndexRangeBuilder { start, end }.build()
    }
}
impl veoveo_types::Check for RecordingVideoSelectionBuilder {
    type Error = anyhow::Error;
    fn check(&self) -> Result<()> {
        validate_selection_fields(self)
    }
}
impl RecordingVideoSelectionBuilder {
    pub fn build(self) -> Result<RecordingVideoSelection> {
        veoveo_types::Checked::new(self).map(RecordingVideoSelection)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RecordingVideoSelection(veoveo_types::Checked<RecordingVideoSelectionBuilder>);
impl schemars::JsonSchema for RecordingVideoSelection {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <RecordingVideoSelectionBuilder as schemars::JsonSchema>::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        <RecordingVideoSelectionBuilder as schemars::JsonSchema>::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <RecordingVideoSelectionBuilder as schemars::JsonSchema>::json_schema(generator)
    }
}

impl std::ops::Deref for RecordingVideoSelection {
    type Target = RecordingVideoSelectionBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl RecordingVideoSelection {
    pub fn into_builder(self) -> RecordingVideoSelectionBuilder {
        self.0.into_inner()
    }
    pub fn new(
        recording_uri: veoveo_recording_contract::RecordingUri,
        entity_path: String,
        timeline: String,
        range: IndexRange,
    ) -> Result<Self> {
        RecordingVideoSelectionBuilder {
            recording_uri,
            entity_path,
            timeline,
            range,
        }
        .build()
    }
}
impl IndexRange {
    pub fn contains(&self, other: IndexRange) -> bool {
        other.start >= self.start && other.end <= self.end
    }
}

/// Timeline kinds a remuxed clip can preserve. Sequence timelines carry no
/// media time and are rejected at materialization mapping.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoTimelineKind {
    DurationNanoseconds,
    TimestampNanoseconds,
}

pub fn validate_video_selection(selection: &RecordingVideoSelection) -> Result<()> {
    validate_selection_fields(selection)
}
fn validate_selection_fields(selection: &RecordingVideoSelectionBuilder) -> Result<()> {
    ensure!(
        selection.range.start <= selection.range.end,
        "video range start must not exceed end"
    );
    ensure!(
        selection.entity_path.starts_with('/')
            && selection.entity_path.len() <= 4_096
            && !selection.entity_path.chars().any(char::is_control),
        "entity_path must be an absolute Rerun path"
    );
    ensure!(
        !selection.timeline.is_empty()
            && selection.timeline.len() <= 256
            && !selection.timeline.chars().any(char::is_control),
        "timeline is invalid"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wire() -> serde_json::Value {
        serde_json::json!({"recording_uri":"recording://recordings/019fa7e9-d7c6-7fe1-bdff-0a5313586c3c",
          "entity_path":"/uav/camera/primary", "timeline":"simulation_time", "range":{"start":-10,"end":10}})
    }
    #[test]
    fn selection_construction_and_decode_share_admission() {
        let value: RecordingVideoSelection = serde_json::from_value(wire()).unwrap();
        validate_video_selection(&value).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), wire());
        assert!(IndexRange::new(5, 4).is_err());
        for (field, invalid) in [
            ("entity_path", "relative"),
            ("entity_path", "/bad\npath"),
            ("timeline", ""),
            ("timeline", "bad\taxis"),
        ] {
            let mut candidate = wire();
            candidate[field] = invalid.into();
            assert!(serde_json::from_value::<RecordingVideoSelection>(candidate).is_err());
        }
        let mut candidate = wire();
        candidate["range"]["end"] = (-11).into();
        assert!(serde_json::from_value::<RecordingVideoSelection>(candidate).is_err());
    }
    #[test]
    fn signed_range_containment_is_preserved() {
        let outer = IndexRange::new(-10, 10).unwrap();
        assert!(outer.contains(IndexRange::new(-3, 7).unwrap()));
        assert!(!outer.contains(IndexRange::new(-11, 7).unwrap()));
    }
}
