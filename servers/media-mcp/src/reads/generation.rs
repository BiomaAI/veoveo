//! Current generation results. Authorization precedes selection and decoding.
use super::{MediaReads, bind_owner};
use crate::contract::{MediaGenerationResult, MediaGenerationUri};
use surrealdb::types::SurrealValue;
use veoveo_platform_store::{OpenObject, RecordId, RecordIdKey, task_record_id};
use veoveo_task_runtime::TaskOwner;
use veoveo_types::TaskId;

#[derive(SurrealValue)]
struct ResultRow {
    task: RecordId,
    result: OpenObject,
}

enum Selection<'a> {
    Resource(&'a MediaGenerationUri),
    Task(TaskId),
}

impl MediaReads<'_> {
    pub async fn generation_result(
        &self,
        owner: &TaskOwner,
        uri: &MediaGenerationUri,
    ) -> anyhow::Result<Option<MediaGenerationResult>> {
        self.select_generation(owner, Selection::Resource(uri))
            .await
    }

    /// Task delivery uses the same visibility and current result decoder
    /// as the result resource, including the current provider-job parent.
    pub async fn generation_for_task(
        &self,
        owner: &TaskOwner,
        task: TaskId,
    ) -> anyhow::Result<Option<MediaGenerationResult>> {
        self.select_generation(owner, Selection::Task(task)).await
    }

    async fn select_generation(
        &self,
        owner: &TaskOwner,
        selection: Selection<'_>,
    ) -> anyhow::Result<Option<MediaGenerationResult>> {
        let statement = match selection {
            Selection::Resource(_) => include_str!("queries/generation_by_prediction.surql"),
            Selection::Task(_) => include_str!("queries/generation_by_task.surql"),
        };
        let query = bind_owner(self.tasks.platform_store().client().query(statement), owner)?;
        let query = match selection {
            Selection::Resource(uri) => query.bind(("prediction", uri.prediction_id().to_string())),
            Selection::Task(task) => query.bind(("task", task_record_id(task))),
        };
        let mut response = query.await?.check()?;
        let rows: Vec<ResultRow> = response.take(0)?;
        rows.into_iter()
            .next()
            .map(|row| {
                let result = decode(row)?;
                let matches = match selection {
                    Selection::Resource(uri) => result.result_uri() == uri,
                    Selection::Task(task) => result.task_id() == task,
                };
                anyhow::ensure!(
                    matches,
                    "stored Media generation result disagrees with its selected parent"
                );
                Ok(result)
            })
            .transpose()
    }
}

fn decode(row: ResultRow) -> anyhow::Result<MediaGenerationResult> {
    anyhow::ensure!(
        row.task.table.as_str() == "task",
        "generation parent is not a Task"
    );
    let RecordIdKey::Uuid(id) = row.task.key else {
        anyhow::bail!("generation parent lacks native Task identity");
    };
    let task_id = TaskId::from_uuid(*id);
    let value = serde_json::Value::Object(row.result.into_map().into_iter().collect());
    #[derive(serde::Deserialize)]
    struct GenerationEnvelope {
        #[serde(rename = "structuredContent")]
        structured_content: MediaGenerationResult,
        #[serde(rename = "isError", default)]
        is_error: Option<bool>,
    }
    let envelope: GenerationEnvelope = serde_json::from_value(value).map_err(|_| {
        anyhow::anyhow!("stored Media generation result requires veoveo.ai/media-generation/v2; drain writers and upgrade Media and consumers together")
    })?;
    anyhow::ensure!(
        envelope.is_error != Some(true),
        "successful generation has an error envelope"
    );
    let generation = envelope.structured_content;
    anyhow::ensure!(
        generation.task_id() == task_id,
        "stored Media generation result disagrees with its selected Task"
    );
    Ok(generation)
}
