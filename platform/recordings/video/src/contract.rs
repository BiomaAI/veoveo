//! Public recorded-video values and validation without source access.

use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod source_snapshot;
pub use source_snapshot::{
    RecordingSourceIdentity, RecordingSourceIdentityKind, RecordingSourceSnapshot,
};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingVideoSelection {
    /// Canonical `recording://recordings/{recording_id}` URI.
    pub recording_uri: veoveo_recording_contract::RecordingUri,
    /// Exact Rerun entity path containing `VideoStream` samples.
    pub entity_path: String,
    /// Rerun duration, timestamp, or sequence timeline.
    pub timeline: String,
    pub range: IndexRange,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexRange {
    pub start: i64,
    pub end: i64,
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

    fn selection() -> RecordingVideoSelection {
        RecordingVideoSelection {
            recording_uri: "recording://recordings/01983da0-0000-7000-8000-000000000000"
                .parse()
                .unwrap(),
            entity_path: "/camera/front".to_owned(),
            timeline: "sensor_time".to_owned(),
            range: IndexRange { start: 0, end: 10 },
        }
    }

    #[test]
    fn selection_validation_rejects_inverted_ranges() {
        let mut inverted = selection();
        inverted.range = IndexRange { start: 5, end: 4 };
        assert!(validate_video_selection(&inverted).is_err());
        assert!(validate_video_selection(&selection()).is_ok());
    }

    #[test]
    fn selection_validation_rejects_relative_entity_paths() {
        let mut relative = selection();
        relative.entity_path = "camera/front".to_owned();
        assert!(validate_video_selection(&relative).is_err());
    }

    #[test]
    fn index_ranges_contain_their_subranges() {
        let outer = IndexRange { start: 0, end: 10 };
        assert!(outer.contains(IndexRange { start: 0, end: 10 }));
        assert!(outer.contains(IndexRange { start: 3, end: 7 }));
        assert!(!outer.contains(IndexRange { start: 3, end: 11 }));
    }
}
