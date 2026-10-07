use super::{
    PipelineId, PipelineUri, SessionCursor, SessionId, SessionPreviewUri, SessionResultsUri,
    SessionUri,
};
// Stream live-session wire types shared with external acceptance clients.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartLiveSessionRequest {
    pub pipeline_id: PipelineId,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StopLiveSessionRequest {
    pub session_id: SessionId,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StopLiveSessionOutput {
    pub result_uri: SessionUri,
    pub lifecycle: LiveSessionLifecycle,
    pub received_video_frames: u64,
    pub processed_frames: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording_output: Option<LiveRecordingOutputView>,
    pub stopped_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveIngressView {
    pub transport: LiveTransport,
    pub host: String,
    pub port: u16,
    pub payload_type: u8,
    pub clock_rate: u32,
    pub caps: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveVideoView {
    /// RFC 6381 AVC codec string admitted with the native pipeline.
    pub codec: String,
    pub width: u16,
    pub height: u16,
    pub frame_rate: u16,
    pub expected_bitrate_bps: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum LiveTransport {
    #[vocabulary(rename = "rtp_h264_udp")]
    RtpH264Udp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum LiveSessionLifecycle {
    #[vocabulary(rename = "starting")]
    Starting,
    #[vocabulary(rename = "running")]
    Running,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "stopped")]
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum LiveRecordingLifecycle {
    #[vocabulary(rename = "starting")]
    Starting,
    #[vocabulary(rename = "forwarding")]
    Forwarding,
    #[vocabulary(rename = "draining")]
    Draining,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "stopped")]
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

/// One authorized page of process-local live sessions, newest IDs first.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveSessionsPage {
    pub sessions: Vec<LiveSessionView>,
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<SessionCursor>,
}

mod start;
pub use start::{LiveStartDetails, StartLiveSessionOutput};

mod view;
pub use view::{LiveSessionDetails, LiveSessionView};
