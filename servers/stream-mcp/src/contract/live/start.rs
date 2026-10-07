//! Derive live addresses from the single admitted session identity.
use super::*;

#[derive(Clone, Debug)]
pub struct LiveStartDetails {
    pub ingress: LiveIngressView,
    pub video: LiveVideoView,
    pub recording_output: Option<LiveRecordingOutputView>,
    pub started_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[serde(
    try_from = "StartLiveSessionOutputWire",
    into = "StartLiveSessionOutputWire"
)]
pub struct StartLiveSessionOutput {
    session: SessionId,
    pub pipeline_uri: PipelineUri,
    pub ingress: LiveIngressView,
    pub video: LiveVideoView,
    pub recording_output: Option<LiveRecordingOutputView>,
    pub started_at: String,
}
impl StartLiveSessionOutput {
    pub fn new(session: SessionId, pipeline: PipelineId, details: LiveStartDetails) -> Self {
        Self {
            session,
            pipeline_uri: PipelineUri::new(pipeline),
            ingress: details.ingress,
            video: details.video,
            recording_output: details.recording_output,
            started_at: details.started_at,
        }
    }
    pub fn session_id(&self) -> SessionId {
        self.session
    }
    pub fn result_uri(&self) -> SessionUri {
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartLiveSessionOutputWire {
    session_id: SessionId,
    result_uri: SessionUri,
    results_uri: SessionResultsUri,
    pipeline_uri: PipelineUri,
    ingress: LiveIngressView,
    video: LiveVideoView,
    preview_uri: SessionPreviewUri,
    #[serde(skip_serializing_if = "Option::is_none")]
    recording_output: Option<LiveRecordingOutputView>,
    started_at: String,
}
impl TryFrom<StartLiveSessionOutputWire> for StartLiveSessionOutput {
    type Error = super::super::StreamContractError;
    fn try_from(wire: StartLiveSessionOutputWire) -> Result<Self, Self::Error> {
        if wire.result_uri.id() != &wire.session_id
            || wire.results_uri.id() != &wire.session_id
            || wire.preview_uri.id() != &wire.session_id
        {
            return Err(Self::Error::InvalidRelationship("session ID and URIs"));
        }
        Ok(Self::new(
            wire.session_id,
            wire.pipeline_uri.id().clone(),
            LiveStartDetails {
                ingress: wire.ingress,
                video: wire.video,
                recording_output: wire.recording_output,
                started_at: wire.started_at,
            },
        ))
    }
}
impl From<StartLiveSessionOutput> for StartLiveSessionOutputWire {
    fn from(view: StartLiveSessionOutput) -> Self {
        Self {
            session_id: view.session_id(),
            result_uri: view.result_uri(),
            results_uri: view.results_uri(),
            preview_uri: view.preview_uri(),
            pipeline_uri: view.pipeline_uri,
            ingress: view.ingress,
            video: view.video,
            recording_output: view.recording_output,
            started_at: view.started_at,
        }
    }
}
