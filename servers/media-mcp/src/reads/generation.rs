//! Retained generation profiles. Authorization precedes selection and decoding.
use super::{MediaReads, VISIBLE_TASK, bind_owner, predictions::VISIBLE_PREDICTION};
use crate::contract::{MediaGenerationResult, MediaGenerationUri, RetainedMediaGeneration};
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

    /// The Task projection uses the same visibility and retained-profile decoder
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
        let predicate = match selection {
            Selection::Resource(_) => "external_job_id = $prediction",
            Selection::Task(_) => "task = $task",
        };
        let query = bind_owner(
            self.tasks.platform_store().client().query(format!(
                "SELECT task, task.result.structuredContent AS result FROM provider_job
             WHERE {VISIBLE_TASK} AND {VISIBLE_PREDICTION} AND {predicate}
               AND task.status = 'succeeded' AND (task.result.isError ?? false) = false
               AND task.result.structuredContent.prediction.id = external_job_id LIMIT 1;"
            )),
            owner,
        )?;
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
    RetainedMediaGeneration::decode(task_id, value)
        .map(RetainedMediaGeneration::into_generation)
        .map_err(|_| anyhow::anyhow!("unsupported or inconsistent stored Media generation result; expected retained v0 or veoveo.ai/media-generation/v1"))
}
