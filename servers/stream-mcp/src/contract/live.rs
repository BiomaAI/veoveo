//! Stream live-session wire types shared with external acceptance clients.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
