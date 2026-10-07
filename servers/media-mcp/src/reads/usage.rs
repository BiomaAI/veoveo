use super::{MediaReads, bind_owner};
use crate::contract::{
    MEDIA_USAGE_PAGE_SIZE, MediaTaskUsageUri, MediaUsageCursor, MediaUsageMetadata, MediaUsagePage,
};
use crate::storage::MediaUsageKind;
use chrono::{DateTime, Utc};
use surrealdb::types::SurrealValue;
use veoveo_mcp_contract::{UsageKind, UsageRecord};
use veoveo_platform_store::{RecordId, RecordIdKey, task_record_id};
use veoveo_task_runtime::TaskOwner;
use veoveo_types::TaskId;

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
    metadata: MediaUsageMetadata,
    recorded_at: DateTime<Utc>,
}

impl MediaReads<'_> {
    pub async fn usage_page(
        &self,
        owner: &TaskOwner,
        cursor: Option<&MediaUsageCursor>,
    ) -> anyhow::Result<MediaUsagePage> {
        let mut response = bind_owner(
            self.tasks
                .platform_store()
                .client()
                .query(if cursor.is_some() {
                    include_str!("queries/usage_page_after.surql")
                } else {
                    include_str!("queries/usage_page.surql")
                }),
            owner,
        )?
        .bind(("after", cursor.map(|cursor| task_record_id(cursor.after()))))
        .bind(("limit", MEDIA_USAGE_PAGE_SIZE + 1))
        .await?
        .check()?;
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
        let mut response = bind_owner(
            self.tasks
                .platform_store()
                .client()
                .query(include_str!("queries/read_usage.surql")),
            owner,
        )?
        .bind(("task", task_record_id(uri.task_id())))
        .await?
        .check()?;
        let rows: Vec<UsageRow> = response.take(0)?;
        rows.into_iter()
            .map(|row| {
                row.kind.require_metadata(&row.metadata)?;
                // A provider ID becomes text only in the shared usage wire model.
                let provider_job_id = row
                    .provider_job_id
                    .map(crate::contract::MediaPredictionId::new)
                    .transpose()?
                    .map(String::from);
                let metadata = serde_json::to_value(row.metadata)?;
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
                    metadata,
                })
            })
            .collect()
    }
}
