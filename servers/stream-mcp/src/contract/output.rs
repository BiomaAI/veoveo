//! A recording run constructs both its run and result addresses.
use super::{
    AnalysisSummary, ModelId, ModelUri, PipelineId, PipelineUri, RunId, RunResultsUri, RunUri,
    StreamContractError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactMetadata;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RunRecordingOutput(veoveo_types::Checked<RunRecordingOutputBuilder>);
impl schemars::JsonSchema for RunRecordingOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <RunRecordingOutputBuilder as schemars::JsonSchema>::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        <RunRecordingOutputBuilder as schemars::JsonSchema>::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <RunRecordingOutputBuilder as schemars::JsonSchema>::json_schema(generator)
    }
}

impl std::ops::Deref for RunRecordingOutput {
    type Target = RunRecordingOutputBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl RunRecordingOutput {
    pub fn into_builder(self) -> RunRecordingOutputBuilder {
        self.0.into_inner()
    }
    pub fn new(
        run: RunId,
        pipeline: PipelineId,
        model: ModelId,
        summary: AnalysisSummary,
        results: ArtifactMetadata,
        annotations: ArtifactMetadata,
    ) -> Result<Self, StreamContractError> {
        RunRecordingOutputBuilder {
            run_uri: RunUri::new(run),
            result_uri: RunResultsUri::new(run),
            pipeline_uri: PipelineUri::new(pipeline),
            model_uri: ModelUri::new(model),
            summary,
            results_artifact: results,
            annotations_artifact: annotations,
            source_clip_artifact: None,
        }
        .build()
    }
    pub fn run_id(&self) -> RunId {
        *self.run_uri.id()
    }
    pub fn result_uri(&self) -> RunResultsUri {
        self.0.result_uri.clone()
    }
    pub fn with_source_clip(
        self,
        clip: Option<ArtifactMetadata>,
    ) -> Result<Self, StreamContractError> {
        let mut value = self.0.into_inner();
        value.source_clip_artifact = clip;
        value.build()
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(rename = "RunRecordingOutput")]
pub struct RunRecordingOutputBuilder {
    pub run_uri: RunUri,
    pub result_uri: RunResultsUri,
    pub pipeline_uri: PipelineUri,
    pub model_uri: ModelUri,
    pub summary: AnalysisSummary,
    pub results_artifact: ArtifactMetadata,
    pub annotations_artifact: ArtifactMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_clip_artifact: Option<ArtifactMetadata>,
}
impl RunRecordingOutputBuilder {
    pub fn build(self) -> Result<RunRecordingOutput, StreamContractError> {
        veoveo_types::Checked::new(self).map(RunRecordingOutput)
    }
}
impl veoveo_types::Check for RunRecordingOutputBuilder {
    type Error = StreamContractError;
    fn check(&self) -> Result<(), Self::Error> {
        super::output_relationships::check(self)
    }
}
