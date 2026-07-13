use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::ArtifactMetadata;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingVideoSelection {
    /// Canonical `recording://recordings/{recording_id}` URI.
    pub recording_uri: String,
    /// Exact Rerun entity path containing `VideoStream` samples.
    pub entity_path: String,
    /// Rerun duration, timestamp, or sequence timeline.
    pub timeline: String,
    pub range: IndexRange,
    #[serde(default)]
    pub source: AnalysisSource,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexRange {
    pub start: i64,
    pub end: i64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AnalysisSource {
    /// Read immutable frozen/sealed RRD segments from the recording spool.
    #[default]
    Durable,
    /// Read the recording hub proxy's bounded recent replay and settle after an
    /// idle interval. This source is intentionally not resumable after a crash.
    RecentProxy {
        #[serde(default = "default_recent_idle_ms")]
        idle_ms: u64,
        #[serde(default = "default_recent_capture_ms")]
        capture_ms: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeRecordingRequest {
    pub video: RecordingVideoSelection,
    pub pipeline_id: String,
    #[serde(default)]
    pub sampling: SamplingPolicy,
    #[serde(default)]
    pub include_source_clip: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
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

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractClipRequest {
    pub video: RecordingVideoSelection,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoundingBox2D {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Detection {
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
pub struct FrameDetections {
    pub index: i64,
    pub detections: Vec<Detection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalysisResults {
    pub schema: String,
    pub pipeline_id: String,
    pub model_id: String,
    pub recording_uri: String,
    pub entity_path: String,
    pub timeline: String,
    pub timeline_kind: VideoTimelineKind,
    pub requested_range: IndexRange,
    pub frames: Vec<FrameDetections>,
    pub processed_frames: u64,
    pub elapsed_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoTimelineKind {
    DurationNanoseconds,
    TimestampNanoseconds,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct AnalyzeRecordingOutput {
    pub analysis_uri: String,
    pub results_uri: String,
    pub pipeline_uri: String,
    pub model_uri: String,
    pub summary: AnalysisSummary,
    pub results_artifact: ArtifactMetadata,
    pub annotations_artifact: ArtifactMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_clip_artifact: Option<ArtifactMetadata>,
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

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExtractClipOutput {
    pub recording_uri: String,
    pub entity_path: String,
    pub timeline: String,
    pub decode_start_index: i64,
    pub requested_range: IndexRange,
    pub sample_count: u64,
    pub artifact: ArtifactMetadata,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PipelineView {
    pub id: String,
    pub uri: String,
    pub title: String,
    pub description: String,
    pub operation: PipelineOperation,
    pub model_uri: String,
    pub tracking: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineOperation {
    ObjectDetection,
    ObjectDetectionTracking,
    InstanceSegmentation,
    PoseEstimation,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ModelView {
    pub id: String,
    pub uri: String,
    pub title: String,
    pub description: String,
    pub format: ModelFormat,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    TensorRtEngine,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct AnalysisView {
    pub analysis_uri: String,
    pub results_uri: String,
    pub task_id: String,
    pub status: String,
    pub progress: f64,
    pub pipeline_id: String,
    pub recording_uri: String,
    pub entity_path: String,
    pub timeline: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<AnalyzeRecordingOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn default_recent_idle_ms() -> u64 {
    750
}

fn default_recent_capture_ms() -> u64 {
    10_000
}
