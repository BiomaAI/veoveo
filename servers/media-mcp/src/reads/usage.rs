use super::{MediaReads, VISIBLE_TASK, bind_owner};
use crate::contract::{MEDIA_USAGE_PAGE_SIZE, MediaTaskUsageUri, MediaUsageCursor, MediaUsagePage};
use chrono::{DateTime, Utc};
use surrealdb::types::SurrealValue;
use veoveo_mcp_contract::{UsageKind, UsageRecord};
use veoveo_platform_store::{MediaUsageKind, OpenObject, RecordId, RecordIdKey, task_record_id};
use veoveo_task_runtime::TaskOwner;
use veoveo_types::TaskId;

const VISIBLE_PROVIDER: &str = "((provider_job ?? NONE) = NONE OR (
    provider_job.tenant = $tenant AND provider_job.task = task
    AND provider_job.provider = $provider
    AND provider_job.external_job_id = provider_job.provider_payload.id))";

#[derive(SurrealValue)]
struct UsageRow {
    provider_job_id: Option<String>,
    source_id: Option<String>,
    model_id: String,
    kind: MediaUsageKind,
    quantity: Option<f64>,
    unit: Option<String>,
    amount: Option<f64>,
    currency: Option<String>,
    metadata: OpenObject,
    recorded_at: DateTime<Utc>,
}

impl MediaReads<'_> {
    pub async fn usage_page(
        &self,
        owner: &TaskOwner,
        cursor: Option<&MediaUsageCursor>,
    ) -> anyhow::Result<MediaUsagePage> {
        let position = if cursor.is_some() {
            "AND task > $after AND task != $after"
        } else {
            ""
        };
        let mut response = bind_owner(self.tasks.platform_store().client().query(format!(
            "SELECT VALUE task FROM media_usage WHERE {VISIBLE_TASK} AND {VISIBLE_PROVIDER} {position} GROUP BY task ORDER BY task ASC LIMIT $limit;"
        )), owner)?
            .bind(("after", cursor.map(|cursor| task_record_id(cursor.after()))))
            .bind(("limit", MEDIA_USAGE_PAGE_SIZE + 1))
            .await?.check()?;
        let mut records: Vec<RecordId> = response.take(0)?;
        let has_more = records.len() > MEDIA_USAGE_PAGE_SIZE;
        records.truncate(MEDIA_USAGE_PAGE_SIZE);
        let ids = records
            .into_iter()
            .map(|record| {
                anyhow::ensure!(
                    record.table.as_str() == "task",
                    "usage parent is not a Task"
                );
                match record.key {
                    RecordIdKey::Uuid(id) => Ok(TaskId::from_uuid(*id)),
                    _ => anyhow::bail!("usage parent lacks native Task identity"),
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let next = has_more.then(|| *ids.last().expect("overfull page is nonempty"));
        Ok(MediaUsagePage::from_task_ids(ids, next)?)
    }

    pub async fn usage(
        &self,
        owner: &TaskOwner,
        uri: &MediaTaskUsageUri,
    ) -> anyhow::Result<Vec<UsageRecord>> {
        let mut response = bind_owner(self.tasks.platform_store().client().query(format!(
            "SELECT provider_job.external_job_id AS provider_job_id, source_id, model_id, kind, quantity, unit, amount, currency, metadata, recorded_at, id FROM media_usage WHERE {VISIBLE_TASK} AND {VISIBLE_PROVIDER} AND task = $task ORDER BY recorded_at ASC, id ASC;"
        )), owner)?.bind(("task", task_record_id(uri.task_id()))).await?.check()?;
        let rows: Vec<UsageRow> = response.take(0)?;
        rows.into_iter()
            .map(|row| {
                // A provider ID becomes text only in the shared usage wire model.
                let provider_job_id = row
                    .provider_job_id
                    .map(crate::contract::MediaPredictionId::new)
                    .transpose()?
                    .map(String::from);
                Ok(UsageRecord {
                    task_id: uri.task_id().to_string(),
                    provider_job_id,
                    source_id: row.source_id,
                    model_id: row.model_id,
                    kind: match row.kind {
                        MediaUsageKind::Estimate => UsageKind::Estimate,
                        MediaUsageKind::Actual => UsageKind::Actual,
                    },
                    quantity: row.quantity,
                    unit: row.unit,
                    amount: row.amount,
                    currency: row.currency,
                    recorded_at: row.recorded_at,
                    metadata: serde_json::Value::Object(
                        row.metadata.into_map().into_iter().collect(),
                    ),
                })
            })
            .collect()
    }
}
