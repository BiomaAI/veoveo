//! Provenance shared by publication and portable result admission.
use super::{ModelId, PipelineId, RunId};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StreamArtifactMetadata {
    pub provenance: StreamArtifactProvenance,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StreamArtifactProvenance {
    #[serde(rename = "stream_results")]
    Results {
        run_id: RunId,
        recording_id: veoveo_recording_mcp::contract::RecordingId,
        pipeline_id: PipelineId,
        model_id: ModelId,
        #[serde(with = "veoveo_types::sha256_hex")]
        #[schemars(schema_with = "digest_schema")]
        source_snapshot_sha256: veoveo_types::Sha256Digest,
    },
    #[serde(rename = "stream_annotation_layer")]
    AnnotationLayer {
        run_id: RunId,
        recording_id: veoveo_recording_mcp::contract::RecordingId,
        results_artifact_uri: veoveo_artifact_contract::ArtifactUri,
        #[serde(with = "veoveo_types::sha256_hex")]
        #[schemars(schema_with = "digest_schema")]
        source_snapshot_sha256: veoveo_types::Sha256Digest,
    },
    #[serde(rename = "stream_source_clip")]
    SourceClip {
        run_id: RunId,
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
    schemars::json_schema!({"type":"string", "pattern":"^[0-9a-f]{64}$"})
}
