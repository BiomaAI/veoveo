use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use veoveo_mcp_contract::{UsageKind, UsageRecord, now_utc};
use veoveo_media_mcp::{
    provider::{BillingRecord, ModelEntry, Prediction},
    state::MediaProviderJob,
    uris,
};

use super::{AppState, BILLING_RECONCILE_INITIAL_DELAY, BILLING_RECONCILE_MAX_DELAY};

fn usage_estimate(task_id: &str, provider_job_id: &str, entry: &ModelEntry) -> UsageRecord {
    UsageRecord {
        task_id: task_id.to_owned(),
        source_id: Some("initial-estimate".into()),
        provider_job_id: Some(provider_job_id.to_owned()),
        model_id: entry.model_id.clone(),
        kind: UsageKind::Estimate,
        quantity: Some(1.0),
        unit: Some("run".into()),
        amount: entry.base_price,
        currency: entry.base_price.map(|_| "USD".into()),
        recorded_at: now_utc(),
        metadata: serde_json::json!({
            "source": "model_registry",
            "model_type": entry.model_type,
            "formula": entry.formula,
            "cost_kind": "estimate"
        }),
    }
}

fn actual_usage_record(
    task_id: &str,
    prediction: &Prediction,
    billing: &BillingRecord,
) -> Option<UsageRecord> {
    #[derive(serde::Serialize)]
    struct ActualUsageMetadata<'a> {
        source: &'static str,
        billing_type: &'a str,
        source_created_at: Option<DateTime<Utc>>,
        source_updated_at: Option<DateTime<Utc>>,
        order_id: Option<&'a str>,
        order_state: Option<&'a str>,
        order_status: Option<&'a str>,
        job_status: Option<&'a str>,
    }

    let amount = billing.signed_amount()?;
    Some(UsageRecord {
        task_id: task_id.to_owned(),
        source_id: Some(billing.uuid.clone()),
        provider_job_id: Some(prediction.id.clone()),
        model_id: billing
            .prediction
            .as_ref()
            .and_then(|prediction| prediction.model_uuid.clone())
            .unwrap_or_else(|| prediction.model.clone()),
        kind: UsageKind::Actual,
        quantity: Some(1.0),
        unit: Some("billing_record".into()),
        amount: Some(amount),
        currency: Some("USD".into()),
        recorded_at: now_utc(),
        metadata: serde_json::to_value(ActualUsageMetadata {
            source: "billing_record",
            billing_type: &billing.billing_type,
            source_created_at: billing.created_at,
            source_updated_at: billing.updated_at,
            order_id: billing
                .order
                .as_ref()
                .and_then(|order| order.uuid.as_deref()),
            order_state: billing
                .order
                .as_ref()
                .and_then(|order| order.state.as_deref()),
            order_status: billing
                .order
                .as_ref()
                .and_then(|order| order.status.as_deref()),
            job_status: billing
                .prediction
                .as_ref()
                .and_then(|prediction| prediction.status.as_deref()),
        })
        .expect("actual usage metadata serializes"),
    })
}

pub(super) async fn record_usage_estimate(
    state: &AppState,
    task_id: &str,
    job: &MediaProviderJob,
    entry: &ModelEntry,
) -> anyhow::Result<()> {
    let task = state
        .tasks
        .get(task_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("media usage task {task_id} not found"))?;
    state
        .durable
        .record_usage(
            &task,
            Some(job),
            &usage_estimate(task_id, &job.external_job_id, entry),
        )
        .await?;
    Ok(())
}

async fn reconcile_actual_usage_once(
    state: &AppState,
    task_id: &str,
    prediction: &Prediction,
) -> anyhow::Result<bool> {
    if state
        .durable
        .has_actual_usage(task_id, &prediction.id)
        .await?
    {
        return Ok(true);
    }
    let task = state
        .tasks
        .get(task_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("media billing task {task_id} not found"))?;
    let job = state
        .durable
        .provider_job_for_external(&prediction.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("media provider job {} not found", prediction.id))?;

    // This endpoint is billing-only. Its response never changes task state and
    // cannot substitute for the signed provider webhook.
    let billing_records = state.provider.billing_records(&prediction.id).await?;
    let mut recorded = 0usize;
    for billing in billing_records {
        let Some(record) = actual_usage_record(task_id, prediction, &billing) else {
            continue;
        };
        state
            .durable
            .record_usage(&task, Some(&job), &record)
            .await?;
        recorded += 1;
    }
    if recorded > 0 {
        state
            .subscribers
            .notify_resource_updated(uris::usage_task_uri(task_id))
            .await;
    }
    Ok(recorded > 0
        || state
            .durable
            .has_actual_usage(task_id, &prediction.id)
            .await?)
}

pub(super) fn spawn_actual_usage_reconciliation(
    state: Arc<AppState>,
    task_id: String,
    prediction: Prediction,
) {
    tokio::spawn(async move {
        let mut delay = Duration::ZERO;
        loop {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            match reconcile_actual_usage_once(&state, &task_id, &prediction).await {
                Ok(true) => break,
                Ok(false) => tracing::info!(
                    task_id,
                    provider_job_id = prediction.id,
                    "actual billing usage is not available yet"
                ),
                Err(error) => tracing::warn!(
                    task_id,
                    provider_job_id = prediction.id,
                    "actual billing reconciliation failed: {error}"
                ),
            }
            delay = if delay.is_zero() {
                BILLING_RECONCILE_INITIAL_DELAY
            } else {
                (delay * 2).min(BILLING_RECONCILE_MAX_DELAY)
            };
        }
    });
}

pub(super) async fn spawn_missing_actual_usage_reconciliations(state: Arc<AppState>) {
    match state.durable.provider_jobs().await {
        Ok(jobs) => {
            for job in jobs {
                if job.prediction.is_terminal()
                    && !state
                        .durable
                        .has_actual_usage(&job.task_id.to_string(), &job.external_job_id)
                        .await
                        .unwrap_or(false)
                {
                    spawn_actual_usage_reconciliation(
                        state.clone(),
                        job.task_id.to_string(),
                        job.prediction,
                    );
                }
            }
        }
        Err(error) => tracing::warn!("failed to enumerate missing billing usage: {error}"),
    }
}
