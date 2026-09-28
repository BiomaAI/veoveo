//! Fixed discovery declarations and typed domain construction.
use crate::contract::{
    AnalysisCursor, AnalysisId, AnalysisUri, ModelId, ModelUri, PipelineId, PipelineUri,
    ReasonDocument, ReasonResource, ResultsUri,
};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_types::{ResourceAddress, ResourceUri};

pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::new("reason").expect("declared server resource scheme")
    });

pub const DOCS_URI: &str = "reason://docs";
pub const CONTRACT_URI: &str = "reason://contract";
pub const DOC_TEMPLATE: &str = "reason://docs/{doc_id}";

pub const PIPELINES_URI: &str = "reason://pipelines";
pub const ANALYSES_APP_URI: &str = "ui://reason/analyses.html";
pub const PIPELINE_TEMPLATE: &str = "reason://pipeline/{pipeline_id}";
pub const MODELS_URI: &str = "reason://models";
pub const MODEL_TEMPLATE: &str = "reason://model/{model_id}";
pub const ANALYSES_URI: &str = "reason://analyses";
pub const ANALYSES_PAGE_TEMPLATE: &str = "reason://analyses{?cursor}";
pub const ANALYSIS_TEMPLATE: &str = "reason://analysis/{analysis_id}";
pub const RESULTS_TEMPLATE: &str = "reason://analysis/{analysis_id}/results";
pub const ARTIFACT_TEMPLATE: &str = "reason://artifact/{artifact_id}";

pub fn doc_uri(id: ReasonDocument) -> ResourceUri {
    ReasonResource::Document(id)
        .to_uri()
        .expect("declared Reason document")
}
pub fn pipeline_uri(id: &PipelineId) -> PipelineUri {
    PipelineUri::new(id.clone())
}
pub fn model_uri(id: &ModelId) -> ModelUri {
    ModelUri::new(id.clone())
}
pub fn analysis_uri(id: AnalysisId) -> AnalysisUri {
    AnalysisUri::new(id)
}
pub fn results_uri(id: AnalysisId) -> ResultsUri {
    ResultsUri::new(id)
}
pub fn analyses_uri(cursor: Option<AnalysisCursor>) -> ResourceUri {
    ReasonResource::Analyses(cursor)
        .to_uri()
        .expect("admitted Reason collection")
}
pub fn artifact_uri(id: ArtifactId) -> ArtifactUri {
    ArtifactUri::presented(&SCHEME, id)
}
