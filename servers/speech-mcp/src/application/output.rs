use super::SpeechService;
use crate::model::{MODEL, MODEL_REVISION};
use anyhow::Result;
use rmcp::model::{CallToolResult, ContentBlock, Resource};
use serde::Serialize;
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactWriteIdempotencyKey, IssuedArtifactWriteCapability,
    PutArtifactRequest, RedeemArtifactWriteCapabilityRequest,
};
use veoveo_speech_contract::transcript::Transcript;
use veoveo_speech_contract::{TranscriptDocument, TranscriptionOutput, transcript_uri};

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct Provenance<'a> {
    source_artifact_uri: &'a str,
    source_sha256: &'a str,
    model: &'a str,
    model_revision: &'a str,
}

impl SpeechService {
    pub(super) async fn publish(
        &self,
        task: &str,
        capability: &IssuedArtifactWriteCapability,
        source: ArtifactMetadata,
        digest: String,
        transcript: Transcript,
    ) -> Result<serde_json::Value> {
        let captions = transcript.webvtt()?.into_bytes();
        let duration_seconds = transcript.duration_seconds;
        let document = TranscriptDocument {
            schema: "veoveo.speech-transcript/v1".into(),
            source_artifact_uri: source.artifact_uri.clone(),
            source_sha256: digest.clone(),
            model: MODEL.into(),
            model_revision: MODEL_REVISION.into(),
            transcript,
        };
        let metadata = serde_json::to_value(Provenance {
            source_artifact_uri: &source.artifact_uri,
            source_sha256: &digest,
            model: MODEL,
            model_revision: MODEL_REVISION,
        })?;
        let mut artifacts = Vec::new();
        for (kind, mime, bytes) in [
            ("json", "application/json", serde_json::to_vec(&document)?),
            ("vtt", "text/vtt", captions),
        ] {
            anyhow::ensure!(
                !self.tasks.is_cancel_requested(task).await?,
                "publication cancelled"
            );
            self.tasks
                .renew_lease(task, std::time::Duration::from_secs(60))
                .await?;
            let request = RedeemArtifactWriteCapabilityRequest {
                capability_id: capability.capability_id,
                task_id: capability.task_id.clone(),
                idempotency_key: ArtifactWriteIdempotencyKey::new(format!("speech:{task}:{kind}"))?,
                artifact: PutArtifactRequest {
                    mime_type: Some(mime.into()),
                    filename: Some(format!("transcript-{task}.{kind}")),
                    classification: source.compliance.classification.clone(),
                    data_labels: source.compliance.data_labels.clone(),
                    retention_expires_at: source.compliance.retention_expires_at,
                    metadata: metadata.clone(),
                },
            };
            artifacts.push(
                self.artifacts
                    .redeem_write_capability(&capability.secret, &request, bytes)
                    .await?
                    .presented_under_scheme("speech"),
            );
        }
        let output = TranscriptionOutput {
            result_uri: transcript_uri(task),
            source_artifact_uri: source.artifact_uri,
            transcript: artifacts.remove(0),
            captions: artifacts.remove(0),
            duration_seconds,
        };
        let mut result = CallToolResult::success(vec![
            ContentBlock::text("Transcript ready."),
            ContentBlock::ResourceLink(
                Resource::new(output.result_uri.clone(), "Transcript")
                    .with_mime_type("application/json"),
            ),
        ]);
        result.structured_content = Some(serde_json::to_value(output)?);
        Ok(serde_json::to_value(result)?)
    }
}
