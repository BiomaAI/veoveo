mod task_kind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use task_kind::StreamTaskKind;

mod cursor;
mod ids;
mod resources;
mod subscriptions;
pub use cursor::{RunCursor, SessionCursor};
pub use ids::{ModelId, PipelineId, RunId, SessionId, StreamContractError};
pub use resources::{
    ModelUri, PipelineUri, RunResultsUri, RunUri, SessionPreviewUri, SessionResultsUri, SessionUri,
    StreamDocument, StreamResource,
};
pub use subscriptions::RunResource;

mod artifact;
mod results;
pub use artifact::{StreamArtifactUri, StreamArtifactUriError};
pub use results::{StreamResultsError, StreamResultsSchema};

pub use veoveo_recording_video::contract::{
    IndexRange, RecordingSourceIdentity, RecordingSourceIdentityKind, RecordingSourceSnapshot,
    RecordingVideoSelection, VideoTimelineKind,
};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunRecordingRequest {
    pub video: RecordingVideoSelection,
    pub pipeline_id: PipelineId,
    #[serde(default)]
    pub sampling: SamplingPolicy,
    #[serde(default)]
    pub include_source_clip: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum SamplingPolicy {
    #[default]
    EveryFrame,
    EveryNth {
        step: u32,
    },
    MaximumFrames {
        count: u32,
    },
}

// Empty wire variants reject undeclared keys while public unit variants stay unchanged.
#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum SamplingPolicyWire {
    EveryFrame {},
    EveryNth { step: u32 },
    MaximumFrames { count: u32 },
}

impl<'de> Deserialize<'de> for SamplingPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match SamplingPolicyWire::deserialize(deserializer)? {
            SamplingPolicyWire::EveryFrame {} => Self::EveryFrame,
            SamplingPolicyWire::EveryNth { step } => Self::EveryNth { step },
            SamplingPolicyWire::MaximumFrames { count } => Self::MaximumFrames { count },
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundingBox2D {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Detection")]
pub struct DetectionBuilder {
    pub class_id: u32,
    pub label: String,
    /// Detector confidence. DeepStream does not provide this value for every
    /// clustering mode or tracker-propagated object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Tracker confidence when the selected tracker exposes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracker_confidence: Option<f32>,
    pub bounds: BoundingBox2D,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_id: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameDetections {
    pub index: i64,
    pub detections: Vec<Detection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "AnalysisResults")]
pub struct AnalysisResultsBuilder {
    pub schema: StreamResultsSchema,
    pub pipeline_id: PipelineId,
    pub model_id: ModelId,
    pub recording_uri: veoveo_recording_mcp::contract::RecordingUri,
    pub entity_path: String,
    pub timeline: String,
    pub timeline_kind: VideoTimelineKind,
    pub requested_range: IndexRange,
    pub source_snapshot: RecordingSourceSnapshot,
    pub frames: Vec<FrameDetections>,
    pub processed_frames: u64,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct AnalysisSummary {
    pub processed_frames: u64,
    pub detection_count: u64,
    pub elapsed_ms: u64,
    pub decode_start_index: i64,
    pub requested_start_index: i64,
    pub requested_end_index: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PipelineProfile {
    PassThrough,
    Perception {
        operation: PerceptionOperation,
        tracking: bool,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionOperation {
    ObjectDetection,
    ObjectDetectionTracking,
    InstanceSegmentation,
    PoseEstimation,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    TensorRtEngine,
}

/// One authorized page of durable recording runs, ordered by creation time and ID.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunPage {
    pub runs: Vec<RunView>,
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<RunCursor>,
}

mod live;
pub use live::*;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LiveResultFrame {
    /// Decode-order identity assigned after the GPU decoder. Presentation timestamps
    /// are intentionally not used because AVC reordering can make them non-monotonic.
    pub index: i64,
    pub observed_at: String,
    pub detections: Vec<Detection>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LiveResultsView {
    pub schema: String,
    pub session_id: SessionId,
    pub pipeline_id: PipelineId,
    pub frames: Vec<LiveResultFrame>,
    pub processed_frames: u64,
    pub dropped_result_frames: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EncodedVideoChunk {
    /// Decode-order identity. This is the ordering contract for retained chunks.
    pub sequence: u64,
    /// H.264 presentation timestamp in microseconds. AVC frame reordering can make
    /// presentation timestamps non-monotonic in decode sequence.
    pub timestamp_us: u64,
    pub keyframe: bool,
    pub data_base64: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LivePreviewView {
    pub schema: String,
    pub session_id: SessionId,
    pub video: LiveVideoView,
    pub chunks: Vec<EncodedVideoChunk>,
    pub dropped_chunks: u64,
    pub received_video_frames: u64,
}

mod catalog_views;
mod output;
mod output_relationships;
mod run_view;
pub use catalog_views::{ModelView, PipelineDetails, PipelineView};
pub use output::{RunRecordingOutput, RunRecordingOutputBuilder};
pub use run_view::{RunDetails, RunView};

mod scopes;
pub use scopes::StreamScope;

impl DetectionBuilder {
    pub fn validate(&self) -> Result<(), StreamResultsError> {
        veoveo_types::Check::check(self)
    }
    pub fn build(self) -> Result<Detection, StreamResultsError> {
        veoveo_types::Checked::new(self).map(Detection)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Detection(veoveo_types::Checked<DetectionBuilder>);
impl schemars::JsonSchema for Detection {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <DetectionBuilder as schemars::JsonSchema>::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        <DetectionBuilder as schemars::JsonSchema>::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <DetectionBuilder as schemars::JsonSchema>::json_schema(generator)
    }
}

impl std::ops::Deref for Detection {
    type Target = DetectionBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl Detection {
    pub fn into_builder(self) -> DetectionBuilder {
        self.0.into_inner()
    }
    pub fn validate(&self) -> Result<(), StreamResultsError> {
        veoveo_types::Check::check(self.0.get())
    }
}

impl AnalysisResultsBuilder {
    pub fn validate(&self) -> Result<(), StreamResultsError> {
        veoveo_types::Check::check(self)
    }
    pub fn build(self) -> Result<AnalysisResults, StreamResultsError> {
        veoveo_types::Checked::new(self).map(AnalysisResults)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct AnalysisResults(veoveo_types::Checked<AnalysisResultsBuilder>);
impl schemars::JsonSchema for AnalysisResults {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <AnalysisResultsBuilder as schemars::JsonSchema>::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        <AnalysisResultsBuilder as schemars::JsonSchema>::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <AnalysisResultsBuilder as schemars::JsonSchema>::json_schema(generator)
    }
}

impl std::ops::Deref for AnalysisResults {
    type Target = AnalysisResultsBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl AnalysisResults {
    pub fn into_builder(self) -> AnalysisResultsBuilder {
        self.0.into_inner()
    }
    pub fn validate(&self) -> Result<(), StreamResultsError> {
        veoveo_types::Check::check(self.0.get())
    }
}

mod artifact_provenance;
pub use artifact_provenance::{StreamArtifactMetadata, StreamArtifactProvenance};

/// Schemas consumed by the server-owned browser App.
pub mod app_schema;

#[cfg(test)]
mod strict_runner_product_tests {
    use super::*;

    #[test]
    fn runner_frame_rejects_nested_additions() {
        let frame = serde_json::json!({"index": 0, "detections": [{"class_id": 1, "label": "person", "bounds": {"x": 0, "y": 0, "width": 1, "height": 1}}]});
        assert!(serde_json::from_value::<FrameDetections>(frame.clone()).is_ok());
        for pointer in ["", "/detections/0", "/detections/0/bounds"] {
            let mut changed = frame.clone();
            changed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), true.into());
            assert!(serde_json::from_value::<FrameDetections>(changed).is_err());
        }
    }
}
