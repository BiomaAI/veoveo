//! Stream's fixed discovery declarations and typed address constructors.
use crate::contract::*;
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_types::{ResourceAddress, ResourceUri};

pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::new("stream").expect("declared server resource scheme")
    });

pub const DOCS_URI: &str = "stream://docs";
pub const CONTRACT_URI: &str = "stream://contract";
pub const DOC_TEMPLATE: &str = "stream://docs/{doc_id}";

pub const PIPELINES_URI: &str = "stream://pipelines";
pub const PIPELINE_TEMPLATE: &str = "stream://pipeline/{pipeline_id}";
pub const MODELS_URI: &str = "stream://models";
pub const MODEL_TEMPLATE: &str = "stream://model/{model_id}";
pub const RUNS_URI: &str = "stream://runs";
pub const RUNS_PAGE_TEMPLATE: &str = "stream://runs{?cursor}";
pub const RUN_TEMPLATE: &str = "stream://run/{run_id}";
pub const RUN_RESULTS_TEMPLATE: &str = "stream://run/{run_id}/results";
pub const SESSIONS_URI: &str = "stream://sessions";
pub const SESSIONS_PAGE_TEMPLATE: &str = "stream://sessions{?cursor}";
pub const SESSION_TEMPLATE: &str = "stream://session/{session_id}";
pub const SESSION_RESULTS_TEMPLATE: &str = "stream://session/{session_id}/results";
pub const SESSION_PREVIEW_TEMPLATE: &str = "stream://session/{session_id}/preview";
pub const ARTIFACT_TEMPLATE: &str = "stream://artifact/{artifact_id}";
pub const LIVE_APP_URI: &str = "ui://stream/live.html";

pub fn doc_uri(id: StreamDocument) -> ResourceUri {
    StreamResource::Document(id)
        .to_uri()
        .expect("declared Stream document")
}
pub fn pipeline_uri(id: &PipelineId) -> PipelineUri {
    PipelineUri::new(id.clone())
}
pub fn model_uri(id: &ModelId) -> ModelUri {
    ModelUri::new(id.clone())
}
pub fn run_uri(id: RunId) -> RunUri {
    RunUri::new(id)
}
pub fn results_uri(id: RunId) -> RunResultsUri {
    RunResultsUri::new(id)
}
pub fn session_uri(id: SessionId) -> SessionUri {
    SessionUri::new(id)
}
pub fn session_results_uri(id: SessionId) -> SessionResultsUri {
    SessionResultsUri::new(id)
}
pub fn session_preview_uri(id: SessionId) -> SessionPreviewUri {
    SessionPreviewUri::new(id)
}
pub fn runs_uri(cursor: Option<RunCursor>) -> ResourceUri {
    StreamResource::Runs(cursor)
        .to_uri()
        .expect("admitted Stream collection")
}
pub fn sessions_uri(cursor: Option<SessionCursor>) -> ResourceUri {
    StreamResource::Sessions(cursor)
        .to_uri()
        .expect("admitted Stream collection")
}
pub fn artifact_uri(id: ArtifactId) -> ArtifactUri {
    ArtifactUri::presented(&SCHEME, id)
}
