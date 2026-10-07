//! One analysis identity constructs both published analysis and result addresses.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactMetadata;

use super::{
    AnalysisId, AnalysisUri, FindingData, ModelUri, PipelineUri, ReasonContractError,
    ReasoningSummary, ResultsUri,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[schemars(transform = analysisoutputschema_format_role)]
pub enum AnalysisOutputSchema {
    #[serde(rename = "veoveo.ai/reason-analysis/v2")]
    V2,
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
    /// Bind the retained product to its actual owner request.
    pub fn check_request(
        &self,
        request: &super::AnalyzeRecordingRequest,
    ) -> Result<(), ReasonContractError> {
        let video = &request.video;
        let finding = &self.finding;
        if finding.pipeline_id() != &request.pipeline_id
            || finding.recording_uri() != &video.recording_uri
            || finding.entity_path() != video.entity_path
            || finding.timeline() != video.timeline
            || finding.requested_range() != video.range
            || finding.task() != &request.task
            || finding.decode() != request.decode
            || self.summary.observed_frames > u64::from(request.sampling.max_frames)
            || self.source_clip_artifact.is_some() != request.include_source_clip
        {
            return Err(ReasonContractError::InvalidRelationship(
                "terminal product and selected request",
            ));
        }
        Ok(())
    }

    /// Admit the full result at the selected Task/Artifact read seam.
    pub fn check_results(
        &self,
        request: &super::AnalyzeRecordingRequest,
        artifact: &ArtifactMetadata,
        results: &super::ReasoningResults,
    ) -> Result<(), ReasonContractError> {
        self.check_request(request)?;
        let invalid = ReasonContractError::InvalidRelationship;
        let video = &request.video;
        if results.pipeline_id != request.pipeline_id
            || results.recording_uri != video.recording_uri
            || results.entity_path != video.entity_path
            || results.timeline != video.timeline
            || results.requested_range != video.range
            || results.task != request.task
            || results.decode != request.decode
            || results.observed_frames > u64::from(request.sampling.max_frames)
        {
            return Err(invalid("results and selected request"));
        }
        if artifact.artifact_id() != self.results_artifact.artifact_id()
            || artifact.artifact_uri != self.results_artifact.artifact_uri
            || artifact.byte_len != self.results_artifact.byte_len
            || artifact.mime_type != self.results_artifact.mime_type
            || artifact.metadata != self.results_artifact.metadata
            || artifact.compliance != self.results_artifact.compliance
        {
            return Err(invalid("selected results Artifact occurrence"));
        }
        let finding =
            super::FindingData::from_results(results).map_err(|_| invalid("results finding"))?;
        if finding != self.finding
            || results.observed_frames != self.summary.observed_frames
            || results.elapsed_ms != self.summary.elapsed_ms
        {
            return Err(invalid("results and terminal finding/summary"));
        }
        Ok(())
    }

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
            schema: AnalysisOutputSchema::V2,
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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

fn analysisoutputschema_format_role(schema: &mut schemars::Schema) {
    *schema = veoveo_types::scalar_schema(
        schema.clone(),
        veoveo_types::ScalarNaming::builtin(veoveo_types::ScalarGrammar::FormatTag),
    )
    .expect("declared Reason format naming profile");
}
