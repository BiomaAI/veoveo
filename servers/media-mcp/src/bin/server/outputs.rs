use axum::http::header::CONTENT_TYPE;
use rmcp::model::CallToolResult;
use veoveo_artifact_contract::{ArtifactMetadata, ArtifactPut};
use veoveo_mcp_contract::{ArtifactWriteIdempotencyKey, now_utc};
use veoveo_media_mcp::{
    contract::{MediaGenerationResult, MediaOutputArtifactMetadata},
    provider::Prediction,
    state::{MediaTaskContext, data_labels},
    task_results::generation_tool_result,
};
use veoveo_types::TaskId;

use super::AppState;

fn guess_mime(url: &str) -> Option<&'static str> {
    let path = url.split('?').next()?;
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => return None,
    })
}

fn filename_from_url(url: &str, index: usize) -> String {
    url.split('?')
        .next()
        .and_then(|p| p.rsplit('/').next())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("output-{index}.bin"))
}

async fn ingest_output_artifact(
    state: &AppState,
    prediction: &Prediction,
    task_id: TaskId,
    context: &MediaTaskContext,
    url: &str,
    index: usize,
) -> anyhow::Result<ArtifactMetadata> {
    let response = state
        .http
        .get(url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("provider output {index} fetch failed: {e}"))?;
    let header_mime = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = response.bytes().await?.to_vec();
    let mut artifact = ArtifactPut::new(bytes);
    artifact.mime_type = header_mime.or_else(|| guess_mime(url).map(str::to_string));
    artifact.filename = Some(filename_from_url(url, index));
    // The plane stamps tenant + owner from the task-bound write capability and
    // records the owner grant; carry labels + retention as put fields.
    artifact.compliance.data_labels = data_labels(&context.owner)?;
    artifact.compliance.retention_expires_at =
        Some(state.retention.artifact_expires_at(now_utc())?);
    artifact.metadata = serde_json::to_value(MediaOutputArtifactMetadata {
        task_id,
        job_id: prediction.id.clone(),
        model_id: prediction.model.clone(),
        output_index: index,
    })?;
    // Async completion has no live gateway bearer. Redeem only the capability
    // issued for this task; media cannot mint or reconstruct an identity.
    let metadata = state
        .artifacts
        .put_with_capability(
            &context.artifact_write_capability,
            ArtifactWriteIdempotencyKey::new(format!("media:{task_id}:output:{index}"))?,
            artifact,
        )
        .await?;
    Ok(metadata)
}

pub(super) async fn prediction_result(
    state: &AppState,
    prediction: &Prediction,
    task_id: TaskId,
    context: &MediaTaskContext,
) -> anyhow::Result<CallToolResult> {
    let mut artifacts = Vec::new();
    for (i, url) in prediction.outputs.iter().enumerate() {
        artifacts.push(ingest_output_artifact(state, prediction, task_id, context, url, i).await?);
    }
    let artifacts = artifacts
        .into_iter()
        .map(ArtifactMetadata::without_download_url)
        .collect::<Vec<_>>();

    generation_tool_result(MediaGenerationResult::new(
        task_id,
        prediction.summary(),
        artifacts,
    )?)
}
