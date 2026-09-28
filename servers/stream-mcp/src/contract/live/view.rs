//! Derive live addresses from the single admitted session identity.
use super::*;

#[derive(Clone, Debug)]
pub struct LiveSessionDetails {
    pub ingress: LiveIngressView,
    pub video: LiveVideoView,
    pub recording_output: Option<LiveRecordingOutputView>,
    pub lifecycle: LiveSessionLifecycle,
    pub started_at: String,
    pub stopped_at: Option<String>,
    pub received_video_frames: u64,
    pub processed_frames: u64,
    pub newest_result_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "LiveSessionViewWire", into = "LiveSessionViewWire")]
pub struct LiveSessionView {
    session: SessionId,
    pub pipeline_uri: PipelineUri,
    pub ingress: LiveIngressView,
    pub video: LiveVideoView,
    pub recording_output: Option<LiveRecordingOutputView>,
    pub lifecycle: LiveSessionLifecycle,
    pub started_at: String,
    pub stopped_at: Option<String>,
    pub received_video_frames: u64,
    pub processed_frames: u64,
    pub newest_result_at: Option<String>,
    pub error: Option<String>,
}
impl LiveSessionView {
    /// Session identity is supplied once to the constructor.
    /// ```compile_fail
    /// use veoveo_stream_mcp::contract::{LiveSessionView, SessionId};
    /// fn detach(view: &mut LiveSessionView, id: SessionId) { view.session_id = id; }
    /// ```
    pub fn new(session: SessionId, pipeline: PipelineId, details: LiveSessionDetails) -> Self {
        Self {
            session,
            pipeline_uri: PipelineUri::new(pipeline),
            ingress: details.ingress,
            video: details.video,
            recording_output: details.recording_output,
            lifecycle: details.lifecycle,
            started_at: details.started_at,
            stopped_at: details.stopped_at,
            received_video_frames: details.received_video_frames,
            processed_frames: details.processed_frames,
            newest_result_at: details.newest_result_at,
            error: details.error,
        }
    }
    pub fn session_id(&self) -> SessionId {
        self.session
    }
    pub fn session_uri(&self) -> SessionUri {
        SessionUri::new(self.session)
    }
    pub fn results_uri(&self) -> SessionResultsUri {
        SessionResultsUri::new(self.session)
    }
    pub fn preview_uri(&self) -> SessionPreviewUri {
        SessionPreviewUri::new(self.session)
    }
    pub fn pipeline_id(&self) -> &PipelineId {
        self.pipeline_uri.id()
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct LiveSessionViewWire {
    session_id: SessionId,
    session_uri: SessionUri,
    results_uri: SessionResultsUri,
    pipeline_id: PipelineId,
    pipeline_uri: PipelineUri,
    ingress: LiveIngressView,
    video: LiveVideoView,
    preview_uri: SessionPreviewUri,
    #[serde(skip_serializing_if = "Option::is_none")]
    recording_output: Option<LiveRecordingOutputView>,
    lifecycle: LiveSessionLifecycle,
    started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stopped_at: Option<String>,
    received_video_frames: u64,
    processed_frames: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    newest_result_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}
impl TryFrom<LiveSessionViewWire> for LiveSessionView {
    type Error = super::super::StreamContractError;
    fn try_from(wire: LiveSessionViewWire) -> Result<Self, Self::Error> {
        if wire.session_uri.id() != &wire.session_id
            || wire.results_uri.id() != &wire.session_id
            || wire.preview_uri.id() != &wire.session_id
        {
            return Err(Self::Error::InvalidRelationship("session ID and URIs"));
        }
        if wire.pipeline_uri.id() != &wire.pipeline_id {
            return Err(Self::Error::InvalidRelationship(
                "session pipeline ID and URI",
            ));
        }
        Ok(Self::new(
            wire.session_id,
            wire.pipeline_uri.id().clone(),
            LiveSessionDetails {
                ingress: wire.ingress,
                video: wire.video,
                recording_output: wire.recording_output,
                lifecycle: wire.lifecycle,
                started_at: wire.started_at,
                stopped_at: wire.stopped_at,
                received_video_frames: wire.received_video_frames,
                processed_frames: wire.processed_frames,
                newest_result_at: wire.newest_result_at,
                error: wire.error,
            },
        ))
    }
}
impl From<LiveSessionView> for LiveSessionViewWire {
    fn from(view: LiveSessionView) -> Self {
        Self {
            session_id: view.session_id(),
            session_uri: view.session_uri(),
            results_uri: view.results_uri(),
            preview_uri: view.preview_uri(),
            pipeline_id: view.pipeline_id().clone(),
            pipeline_uri: view.pipeline_uri,
            ingress: view.ingress,
            video: view.video,
            recording_output: view.recording_output,
            lifecycle: view.lifecycle,
            started_at: view.started_at,
            stopped_at: view.stopped_at,
            received_video_frames: view.received_video_frames,
            processed_frames: view.processed_frames,
            newest_result_at: view.newest_result_at,
            error: view.error,
        }
    }
}
