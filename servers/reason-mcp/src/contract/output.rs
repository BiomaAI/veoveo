//! One analysis identity constructs both published analysis and result addresses.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactMetadata;

use super::{
    AnalysisId, AnalysisUri, FindingData, ModelUri, PipelineUri, ReasonContractError,
    ReasoningSummary, ResultsUri,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
pub enum AnalysisOutputSchema {
    #[serde(rename = "veoveo.ai/reason-analysis/v1")]
    V1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct AnalyzeRecordingOutput(veoveo_types::Checked<AnalyzeRecordingOutputBuilder>);
impl schemars::JsonSchema for AnalyzeRecordingOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <AnalyzeRecordingOutputBuilder as schemars::JsonSchema>::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        <AnalyzeRecordingOutputBuilder as schemars::JsonSchema>::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <AnalyzeRecordingOutputBuilder as schemars::JsonSchema>::json_schema(generator)
    }
}

impl std::ops::Deref for AnalyzeRecordingOutput {
    type Target = AnalyzeRecordingOutputBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl AnalyzeRecordingOutput {
    pub fn into_builder(self) -> AnalyzeRecordingOutputBuilder {
        self.0.into_inner()
    }
    pub fn new(
        analysis: AnalysisId,
        finding: FindingData,
        summary: ReasoningSummary,
        results: ArtifactMetadata,
        annotations: ArtifactMetadata,
    ) -> Result<Self, ReasonContractError> {
        AnalyzeRecordingOutputBuilder {
            schema: AnalysisOutputSchema::V1,
            analysis_uri: AnalysisUri::new(analysis),
            result_uri: ResultsUri::new(analysis),
            pipeline_uri: PipelineUri::new(finding.pipeline_id().clone()),
            model_uri: ModelUri::new(finding.model_id().clone()),
            finding,
            summary,
            results_artifact: results,
            annotations_artifact: annotations,
            source_clip_artifact: None,
        }
        .build()
    }
    pub fn analysis_id(&self) -> AnalysisId {
        *self.analysis_uri.id()
    }
    pub fn result_uri(&self) -> ResultsUri {
        self.0.result_uri.clone()
    }
    pub fn with_source_clip(
        self,
        clip: Option<ArtifactMetadata>,
    ) -> Result<Self, ReasonContractError> {
        let mut value = self.0.into_inner();
        value.source_clip_artifact = clip;
        value.build()
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "AnalyzeRecordingOutput")]
pub struct AnalyzeRecordingOutputBuilder {
    pub schema: AnalysisOutputSchema,
    pub analysis_uri: AnalysisUri,
    pub result_uri: ResultsUri,
    pub pipeline_uri: PipelineUri,
    pub model_uri: ModelUri,
    pub summary: ReasoningSummary,
    pub finding: FindingData,
    pub results_artifact: ArtifactMetadata,
    pub annotations_artifact: ArtifactMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_clip_artifact: Option<ArtifactMetadata>,
}
impl AnalyzeRecordingOutputBuilder {
    pub fn build(self) -> Result<AnalyzeRecordingOutput, ReasonContractError> {
        veoveo_types::Checked::new(self).map(AnalyzeRecordingOutput)
    }
}
impl veoveo_types::Check for AnalyzeRecordingOutputBuilder {
    type Error = ReasonContractError;
    fn check(&self) -> Result<(), Self::Error> {
        super::output_relationships::check(self)
    }
}
