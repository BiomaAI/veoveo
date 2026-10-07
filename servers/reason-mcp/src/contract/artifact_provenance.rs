//! Stored Reason Artifact provenance, shared by publication and resource readers.
use super::{AnalysisId, ModelId, PipelineId, ReasoningTask};

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ReasoningKind {
    #[vocabulary(rename = "describe_segment")]
    DescribeSegment,
    #[vocabulary(rename = "detect_events")]
    DetectEvents,
    #[vocabulary(rename = "answer_question")]
    AnswerQuestion,
}
impl From<&ReasoningTask> for ReasoningKind {
    fn from(task: &ReasoningTask) -> Self {
        match task {
            ReasoningTask::DescribeSegment { .. } => Self::DescribeSegment,
            ReasoningTask::DetectEvents { .. } => Self::DetectEvents,
            ReasoningTask::AnswerQuestion { .. } => Self::AnswerQuestion,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReasonArtifactMetadata {
    pub provenance: ReasonArtifactProvenance,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReasonArtifactProvenance {
    #[serde(rename = "reason_results")]
    Results {
        analysis_id: AnalysisId,
        recording_id: veoveo_recording_mcp::contract::RecordingId,
        pipeline_id: PipelineId,
        model_id: ModelId,
        prompt_revision: String,
        task_kind: ReasoningKind,
        #[serde(with = "veoveo_types::sha256_hex")]
        #[schemars(schema_with = "digest_schema")]
        source_snapshot_sha256: veoveo_types::Sha256Digest,
    },
    #[serde(rename = "reason_annotation_layer")]
    AnnotationLayer {
        analysis_id: AnalysisId,
        recording_id: veoveo_recording_mcp::contract::RecordingId,
        results_artifact_uri: veoveo_artifact_contract::ArtifactUri,
        #[serde(with = "veoveo_types::sha256_hex")]
        #[schemars(schema_with = "digest_schema")]
        source_snapshot_sha256: veoveo_types::Sha256Digest,
    },
    #[serde(rename = "reason_source_clip")]
    SourceClip {
        analysis_id: AnalysisId,
        recording_id: veoveo_recording_mcp::contract::RecordingId,
        entity_path: String,
        timeline: String,
        decode_start_index: i64,
        #[serde(with = "veoveo_types::sha256_hex")]
        #[schemars(schema_with = "digest_schema")]
        source_snapshot_sha256: veoveo_types::Sha256Digest,
    },
}

fn digest_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}
