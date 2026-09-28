//! Media ledger writes, retention and SQL selection for billing recovery.
use super::{
    MEDIA_EVENT_SCHEMA_VERSION, MediaProviderJob, MediaState, PROVIDER, STATE_ID_NAMESPACE,
    open_object, provider_job, tenant_record,
};
use crate::contract::MediaPredictionId;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_mcp_contract::{UsageKind, UsageRecord};
use veoveo_platform_store::{
    MediaUsageId, MediaUsageKind, MediaUsageRecord, OpenObject, OutboxDraft, ProviderJobId,
    ProviderJobRecord, StoreError, task_record_id,
};
use veoveo_task_runtime::TaskSnapshot;
use veoveo_types::TaskId;

pub struct MediaBillingPage {
    pub jobs: Vec<MediaProviderJob>,
    pub next_job_id: Option<ProviderJobId>,
}

impl MediaState {
    pub async fn record_usage(
        &self,
        task: &TaskSnapshot,
        provider_job: Option<&MediaProviderJob>,
        usage: &UsageRecord,
    ) -> Result<(), StoreError> {
        if usage.task_id != task.task_id.to_string()
            || provider_job.is_some_and(|job| job.task_id != task.task_id)
            || usage.provider_job_id.as_deref()
                != provider_job.map(|job| job.external_job_id.as_str())
        {
            return Err(StoreError::ArtifactWriteConflict {
                key: task.task_id.to_string(),
            });
        }
        let kind = match usage.kind {
            UsageKind::Estimate => MediaUsageKind::Estimate,
            UsageKind::Actual => MediaUsageKind::Actual,
        };
        let key = format!(
            "{}:{}:{}",
            task.task_id,
            match kind {
                MediaUsageKind::Estimate => "estimate",
                MediaUsageKind::Actual => "actual",
            },
            usage.source_id.as_deref().unwrap_or("initial")
        );
        let id = MediaUsageId::from_uuid(Uuid::new_v5(&STATE_ID_NAMESPACE, key.as_bytes()));
        let record = MediaUsageRecord {
            id: id.record_id(),
            tenant: tenant_record(&task.owner)?,
            task: task_record_id(task.task_id),
            provider_job: provider_job.map(|job| job.job_id.record_id()),
            source_id: usage.source_id.clone(),
            model_id: usage.model_id.clone(),
            kind,
            quantity: usage.quantity,
            unit: usage.unit.clone(),
            amount: usage.amount,
            currency: usage.currency.clone(),
            metadata: open_object(usage.metadata.clone()),
            recorded_at: usage.recorded_at,
        };
        let outbox = OutboxDraft::now(
            Some(tenant_record(&task.owner)?),
            "media_usage",
            id.to_string(),
            "media.usage.recorded",
            MEDIA_EVENT_SCHEMA_VERSION,
            OpenObject::new(BTreeMap::from([(
                "task_id".into(),
                serde_json::json!(task.task_id.to_string()),
            )])),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; UPSERT ONLY $usage CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("usage", id.record_id()))
            .bind(("content", record))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn has_actual_usage(
        &self,
        task: TaskId,
        external_job_id: &MediaPredictionId,
    ) -> Result<bool, StoreError> {
        let mut response = self.store.client()
            .query("RETURN count((SELECT VALUE id FROM media_usage WHERE task = $task AND tenant = task.tenant AND task.server = mcp_server:media AND provider_job.task = task AND provider_job.tenant = tenant AND provider_job.provider = $provider AND provider_job.external_job_id = $prediction AND provider_job.provider_payload.id = $prediction AND kind = 'actual' LIMIT 1)) > 0;")
            .bind(("task", task_record_id(task)))
            .bind(("provider", PROVIDER.to_owned()))
            .bind(("prediction", external_job_id.to_string()))
            .await?.check()?;
        Ok(response.take::<Option<bool>>(0)?.unwrap_or(false))
    }

    pub async fn delete_usage_before(&self, cutoff: DateTime<Utc>) -> Result<u64, StoreError> {
        let mut response = self
            .store
            .client()
            .query("DELETE media_usage WHERE recorded_at < $cutoff RETURN BEFORE;")
            .bind(("cutoff", cutoff))
            .await?
            .check()?;
        Ok(response.take::<Vec<MediaUsageRecord>>(0)?.len() as u64)
    }

    /// Internal recovery: terminal, correctly linked provider jobs without actual billing.
    /// Provider IDs are tenant-scoped; native Store job identity orders this cursor.
    pub async fn billing_candidates(
        &self,
        after: Option<ProviderJobId>,
    ) -> Result<MediaBillingPage, StoreError> {
        let position = if after.is_some() {
            "AND id > $after AND id != $after"
        } else {
            ""
        };
        let mut response = self.store.client().query(format!(
            "SELECT * FROM provider_job WHERE provider = $provider AND task.server = mcp_server:media AND tenant = task.tenant AND external_job_id = provider_payload.id AND provider_payload.status IN ['completed', 'failed'] AND (SELECT VALUE id FROM media_usage WHERE task = $parent.task AND tenant = $parent.tenant AND provider_job = $parent.id AND kind = 'actual' LIMIT 1) = [] {position} ORDER BY id ASC LIMIT 101;"
        )).bind(("provider", PROVIDER.to_owned())).bind(("after", after.map(|id| id.record_id()))).await?.check()?;
        let mut records: Vec<ProviderJobRecord> = response.take(0)?;
        let has_more = records.len() > 100;
        records.truncate(100);
        let jobs = records
            .into_iter()
            .map(provider_job)
            .collect::<Result<Vec<_>, _>>()?;
        let next_job_id = has_more.then(|| jobs.last().expect("overfull page has jobs").job_id);
        Ok(MediaBillingPage { jobs, next_job_id })
    }
}
