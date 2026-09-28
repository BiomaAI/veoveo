//! Retained generation profiles. Authorization precedes selection and decoding.
use super::{MediaReads, VISIBLE_TASK, bind_owner, predictions::VISIBLE_PREDICTION};
use crate::contract::{GenerationPredictionSummary, MediaGenerationResult, MediaGenerationUri};
use serde::Deserialize;
use surrealdb::types::SurrealValue;
use veoveo_artifact_contract::ArtifactMetadata;
use veoveo_platform_store::{OpenObject, RecordId, RecordIdKey};
use veoveo_task_runtime::TaskOwner;
use veoveo_types::TaskId;

#[derive(SurrealValue)]
struct ResultRow {
    task: RecordId,
    result: OpenObject,
}

// Explicit retained-data adapter for the pre-v1 Media tool result. An unknown
// version or incomplete v1 cannot fall through to this closed unversioned shape.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerationResultV0 {
    prediction: GenerationPredictionSummary,
    artifacts: Vec<ArtifactMetadata>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredResult {
    V1(MediaGenerationResult),
    V0(GenerationResultV0),
}

impl MediaReads<'_> {
    pub async fn generation_result(
        &self,
        owner: &TaskOwner,
        uri: &MediaGenerationUri,
    ) -> anyhow::Result<Option<MediaGenerationResult>> {
        let mut response = bind_owner(self.tasks.platform_store().client().query(format!(
            "SELECT task, task.result.structuredContent AS result FROM provider_job WHERE {VISIBLE_TASK} AND {VISIBLE_PREDICTION} AND external_job_id = $prediction AND task.status = 'succeeded' AND (task.result.isError ?? false) = false AND task.result.structuredContent.prediction.id = external_job_id LIMIT 1;"
        )), owner)?.bind(("prediction", uri.prediction_id().to_string())).await?.check()?;
        let rows: Vec<ResultRow> = response.take(0)?;
        rows.into_iter()
            .next()
            .map(|row| decode(row, uri))
            .transpose()
    }
}

fn decode(row: ResultRow, uri: &MediaGenerationUri) -> anyhow::Result<MediaGenerationResult> {
    anyhow::ensure!(
        row.task.table.as_str() == "task",
        "generation parent is not a Task"
    );
    let RecordIdKey::Uuid(id) = row.task.key else {
        anyhow::bail!("generation parent lacks native Task identity");
    };
    let task_id = TaskId::from_uuid(*id);
    let value = serde_json::Value::Object(row.result.into_map().into_iter().collect());
    let result = match serde_json::from_value::<StoredResult>(value).map_err(|_| anyhow::anyhow!("unsupported or inconsistent stored Media generation result; expected retained v0 or veoveo.ai/media-generation/v1"))? {
        StoredResult::V1(result) => result,
        StoredResult::V0(legacy) => MediaGenerationResult::new(task_id, legacy.prediction, legacy.artifacts)?,
    };
    anyhow::ensure!(
        result.task_id() == task_id && result.result_uri() == uri,
        "stored Media generation result disagrees with its Task or prediction parent"
    );
    Ok(result)
}
