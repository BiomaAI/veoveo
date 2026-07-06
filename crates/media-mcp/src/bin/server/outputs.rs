use axum::http::header::CONTENT_TYPE;
use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactPut, GenerationPredictionSummary, GenerationRunOutput, now_utc,
    set_related_task_meta,
};
use veoveo_media_mcp::{provider::Prediction, state::TaskOwner};

use super::{AppState, ownership::artifact_owner_from_task};

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

pub(super) fn public_prediction(prediction: &Prediction) -> GenerationPredictionSummary {
    GenerationPredictionSummary {
        id: prediction.id.clone(),
        model_id: prediction.model.clone(),
        status: prediction.status.clone(),
        created_at: prediction.created_at,
        error: prediction.error.clone().filter(|error| !error.is_empty()),
        execution_ms: prediction.execution_time,
        timings: prediction.timings.clone(),
        output_count: prediction.outputs.len(),
    }
}

#[derive(serde::Serialize)]
struct OutputArtifactMetadata<'a> {
    task_id: &'a str,
    job_id: &'a str,
    model_id: &'a str,
    output_index: usize,
}

async fn ingest_output_artifact(
    state: &AppState,
    prediction: &Prediction,
    task_id: &str,
    owner: &TaskOwner,
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
    artifact.compliance.owner_id = Some(owner.principal_id.clone());
    artifact.compliance.tenant_id = owner.tenant.clone();
    artifact.compliance.data_labels = owner.data_labels.clone();
    artifact.compliance.retention_expires_at =
        Some(state.retention.artifact_expires_at(now_utc())?);
    artifact.metadata = serde_json::to_value(OutputArtifactMetadata {
        task_id,
        job_id: prediction.id.as_str(),
        model_id: prediction.model.as_str(),
        output_index: index,
    })?;
    let mut metadata = state.artifacts.put(artifact).await?;
    let artifact_owner = artifact_owner_from_task(&metadata.sha256, owner);
    state.durable.record_artifact_owner(&artifact_owner)?;
    metadata.compliance.owner_id = Some(owner.principal_id.clone());
    metadata.compliance.tenant_id = owner.tenant.clone();
    metadata.compliance.data_labels = owner.data_labels.clone();
    Ok(metadata)
}

pub(super) async fn prediction_result(
    state: &AppState,
    prediction: &Prediction,
    task_id: &str,
    owner: &TaskOwner,
) -> anyhow::Result<CallToolResult> {
    let mut artifacts = Vec::new();
    for (i, url) in prediction.outputs.iter().enumerate() {
        artifacts.push(ingest_output_artifact(state, prediction, task_id, owner, url, i).await?);
    }
    let artifacts = artifacts
        .into_iter()
        .map(ArtifactMetadata::without_download_url)
        .collect::<Vec<_>>();

    let mut blocks = vec![ContentBlock::text(format!(
        "prediction {} ({}) completed with {} artifact(s) in {:.1}s",
        prediction.id,
        prediction.model,
        artifacts.len(),
        prediction.execution_time.unwrap_or_default() / 1000.0,
    ))];
    for (i, artifact) in artifacts.iter().enumerate() {
        let mut link = Resource::new(artifact.artifact_uri.clone(), format!("output-{i}"))
            .with_description(format!("artifact {i} of prediction {}", prediction.id));
        if let Some(mime) = &artifact.mime_type {
            link = link.with_mime_type(mime.clone());
        }
        blocks.push(ContentBlock::ResourceLink(link));
    }
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(GenerationRunOutput {
        prediction: public_prediction(prediction),
        artifacts,
    })?);
    set_related_task_meta(&mut result.meta, task_id);
    Ok(result)
}
