use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::ArtifactMetadata;

pub use veoveo_recording_video::{IndexRange, RecordingVideoSelection, VideoTimelineKind};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunRecordingRequest {
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

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct RunRecordingOutput {
    pub run_uri: String,
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

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PipelineView {
    pub id: String,
    pub uri: String,
    pub title: String,
    pub description: String,
    pub profile: PipelineProfile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_uri: Option<String>,
    pub supports_recording_replay: bool,
    pub supports_live_input: bool,
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
pub struct RunView {
    pub run_uri: String,
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
    pub output: Option<RunRecordingOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartLiveSessionRequest {
    pub pipeline_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StopLiveSessionRequest {
    pub session_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct StartLiveSessionOutput {
    pub session_id: String,
    pub session_uri: String,
    pub results_uri: String,
    pub pipeline_uri: String,
    pub ingress: LiveIngressView,
    pub video: LiveVideoView,
    pub preview_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording_output: Option<LiveRecordingOutputView>,
    pub started_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct StopLiveSessionOutput {
    pub session_uri: String,
    pub lifecycle: LiveSessionLifecycle,
    pub received_video_frames: u64,
    pub processed_frames: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording_output: Option<LiveRecordingOutputView>,
    pub stopped_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LiveIngressView {
    pub transport: LiveTransport,
    pub host: String,
    pub port: u16,
    pub payload_type: u8,
    pub clock_rate: u32,
    pub caps: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LiveVideoView {
    /// RFC 6381 AVC codec string admitted with the native pipeline.
    pub codec: String,
    pub width: u16,
    pub height: u16,
    pub frame_rate: u16,
    pub expected_bitrate_bps: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LiveTransport {
    RtpH264Udp,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LiveSessionLifecycle {
    Starting,
    Running,
    Failed,
    Stopped,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LiveRecordingLifecycle {
    Starting,
    Forwarding,
    Draining,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LiveRecordingOutputView {
    pub recording_key: String,
    pub application_id: String,
    pub entity_path: String,
    pub timeline: String,
    pub lifecycle: LiveRecordingLifecycle,
    pub forwarded_video_frames: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LiveSessionView {
    pub session_id: String,
    pub session_uri: String,
    pub results_uri: String,
    pub pipeline_id: String,
    pub pipeline_uri: String,
    pub ingress: LiveIngressView,
    pub video: LiveVideoView,
    pub preview_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording_output: Option<LiveRecordingOutputView>,
    pub lifecycle: LiveSessionLifecycle,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped_at: Option<String>,
    pub received_video_frames: u64,
    pub processed_frames: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newest_result_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LiveResultFrame {
    pub index: i64,
    pub observed_at: String,
    pub detections: Vec<Detection>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LiveResultsView {
    pub schema: String,
    pub session_id: String,
    pub pipeline_id: String,
    pub frames: Vec<LiveResultFrame>,
    pub processed_frames: u64,
    pub dropped_result_frames: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct EncodedVideoChunk {
    pub sequence: u64,
    pub timestamp_us: u64,
    pub keyframe: bool,
    pub data_base64: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct LivePreviewView {
    pub schema: String,
    pub session_id: String,
    pub video: LiveVideoView,
    pub chunks: Vec<EncodedVideoChunk>,
    pub dropped_chunks: u64,
    pub received_video_frames: u64,
}
