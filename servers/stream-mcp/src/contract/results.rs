//! Portable replay-result checks shared by producers and contract-only readers.
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{AnalysisResults, Detection, RecordingVideoSelection};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
pub enum StreamResultsSchema {
    #[serde(rename = "veoveo.stream-results/v1")]
    V1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamResultsError {
    Selection,
    FrameCount,
    FrameOrder,
    FrameRange,
    ClassId,
    Label,
    Confidence,
    Bounds,
}
impl fmt::Display for StreamResultsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Selection => "invalid Stream replay selection",
            Self::FrameCount => "Stream processed frame count is smaller than returned frames",
            Self::FrameOrder => "Stream result frames must be strictly ordered",
            Self::FrameRange => "Stream result frame is outside the requested range",
            Self::ClassId => "Stream detection class ID exceeds the supported u16 profile",
            Self::Label => "Stream detection label must contain 1–256 bytes of nonblank text",
            Self::Confidence => "Stream detection confidence must be finite and within 0..=1",
            Self::Bounds => {
                "Stream detection bounds must be finite with nonnegative origin and positive size"
            }
        })
    }
}
impl std::error::Error for StreamResultsError {}

impl AnalysisResults {
    /// Check the relationships expressible in a published replay document.
    /// The producer separately checks bounds against its private input dimensions.
    pub fn validate(&self) -> Result<(), StreamResultsError> {
        let StreamResultsSchema::V1 = self.schema;
        veoveo_recording_video::contract::validate_video_selection(&RecordingVideoSelection {
            recording_uri: self.recording_uri.clone(),
            entity_path: self.entity_path.clone(),
            timeline: self.timeline.clone(),
            range: self.requested_range,
        })
        .map_err(|_| StreamResultsError::Selection)?;
        if self.processed_frames < self.frames.len() as u64 {
            return Err(StreamResultsError::FrameCount);
        }
        let mut previous = None;
        for frame in &self.frames {
            if frame.index < self.requested_range.start || frame.index > self.requested_range.end {
                return Err(StreamResultsError::FrameRange);
            }
            if previous.is_some_and(|index| frame.index <= index) {
                return Err(StreamResultsError::FrameOrder);
            }
            previous = Some(frame.index);
            for detection in &frame.detections {
                detection.validate()?;
            }
        }
        Ok(())
    }
}

impl Detection {
    pub fn validate(&self) -> Result<(), StreamResultsError> {
        if u16::try_from(self.class_id).is_err() {
            return Err(StreamResultsError::ClassId);
        }
        if self.label.trim().is_empty() || self.label.len() > 256 {
            return Err(StreamResultsError::Label);
        }
        for confidence in [self.confidence, self.tracker_confidence]
            .into_iter()
            .flatten()
        {
            if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
                return Err(StreamResultsError::Confidence);
            }
        }
        let b = &self.bounds;
        if ![b.x, b.y, b.width, b.height]
            .into_iter()
            .all(f32::is_finite)
            || b.x < 0.0
            || b.y < 0.0
            || b.width <= 0.0
            || b.height <= 0.0
        {
            return Err(StreamResultsError::Bounds);
        }
        Ok(())
    }
}
