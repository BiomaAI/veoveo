pub mod dictation;
pub mod transcript;
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use transcript::Transcript;
use veoveo_mcp_contract::{ArtifactId, ArtifactMetadata, parse_artifact_plane_uri};

pub const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const TRANSCRIPT_MIME: &str = "application/json";

#[derive(JsonSchema)]
#[allow(dead_code)]
struct SchemaBundle {
    start_dictation: dictation::StartDictation,
    dictation_id: dictation::DictationId,
    dictation: dictation::DictationSnapshot,
    transcribe: TranscribeRequest,
    output: TranscriptionOutput,
    document: TranscriptDocument,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(SchemaBundle)
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranscribeRequest {
    /// `artifact://` URI of an uploaded audio or video file you can read.
    pub artifact_uri: String,
}

impl TranscribeRequest {
    pub fn source(&self) -> Result<ArtifactId> {
        ensure!(self.artifact_uri.len() <= 256, "source URI exceeds limit");
        parse_artifact_plane_uri(&self.artifact_uri)
            .ok_or_else(|| anyhow::anyhow!("a canonical Artifact URI is required"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionOutput {
    pub result_uri: String,
    pub source_artifact_uri: String,
    pub transcript: ArtifactMetadata,
    pub captions: ArtifactMetadata,
    pub duration_seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranscriptDocument {
    pub schema: String,
    pub source_artifact_uri: String,
    pub source_sha256: String,
    pub model: String,
    pub model_revision: String,
    pub transcript: Transcript,
}

pub fn transcript_uri(task: &str) -> String {
    format!("speech://transcript/{task}")
}

pub fn validate_source(metadata: &ArtifactMetadata) -> Result<()> {
    ensure!(
        metadata.byte_len > 0 && metadata.byte_len <= MAX_SOURCE_BYTES,
        "source must be nonempty and at most 2 GiB"
    );
    let mime = metadata
        .mime_type
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default();
    ensure!(
        mime.starts_with("audio/") || mime.starts_with("video/"),
        "source must be audio or video"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_network_and_local_source_authority() {
        for uri in [
            "https://example.com/audio.wav",
            "file:///etc/passwd",
            "speech://transcript/anything",
        ] {
            assert!(
                TranscribeRequest {
                    artifact_uri: uri.into()
                }
                .source()
                .is_err()
            );
        }
        let id = ArtifactId::new();
        assert_eq!(
            TranscribeRequest {
                artifact_uri: id.plane_uri()
            }
            .source()
            .unwrap(),
            id
        );
    }
}
