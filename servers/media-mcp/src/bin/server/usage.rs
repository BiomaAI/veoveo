use std::{sync::Arc, time::Duration};

use veoveo_mcp_contract::{UsageKind, UsageRecord, now_utc};
use veoveo_media_mcp::{
    contract::{MediaPredictionId, MediaTaskUsageUri},
    contract::{MediaUsageCostKind, MediaUsageMetadata, ModelEntry},
    provider::{BillingRecord, Prediction},
    state::MediaProviderJob,
};

use super::{AppState, BILLING_RECONCILE_INITIAL_DELAY, BILLING_RECONCILE_MAX_DELAY};
use veoveo_types::TaskId;

fn usage_estimate(
    task_id: TaskId,
    provider_job_id: &MediaPredictionId,
    entry: &ModelEntry,
) -> UsageRecord {
    UsageRecord {
        task_id: task_id.to_string(),
        source_id: Some("initial-estimate".into()),
        provider_job_id: Some(provider_job_id.to_string()),
        model_id: entry.model_id.to_string(),
        kind: UsageKind::Estimate,
        quantity: Some(1.0),
        unit: Some("run".into()),
        amount: entry.base_price,
        currency: entry.base_price.map(|_| "USD".into()),
        recorded_at: now_utc(),
        metadata: serde_json::to_value(MediaUsageMetadata::ModelRegistry {
            model_type: entry.model_type.clone(),
            formula: entry.formula.clone(),
            cost_kind: MediaUsageCostKind::Estimate,
        })
        .expect("estimate usage metadata serializes"),
    }
}

fn actual_usage_record(
    task_id: TaskId,
    prediction: &Prediction,
    billing: &BillingRecord,
) -> Option<UsageRecord> {
    let amount = billing.signed_amount()?;
    Some(UsageRecord {
        task_id: task_id.to_string(),
        source_id: Some(billing.uuid.clone()),
        provider_job_id: Some(prediction.id.to_string()),
        model_id: billing
            .prediction
            .as_ref()
            .and_then(|prediction| prediction.model_uuid.clone())
            .unwrap_or_else(|| prediction.model.to_string()),
        kind: UsageKind::Actual,
        quantity: Some(1.0),
        unit: Some("billing_record".into()),
        amount: Some(amount),
        currency: Some("USD".into()),
        recorded_at: now_utc(),
        metadata: serde_json::to_value(MediaUsageMetadata::BillingRecord {
            billing_type: billing.billing_type.clone(),
            source_created_at: billing.created_at,
            source_updated_at: billing.updated_at,
            order_id: billing.order.as_ref().and_then(|order| order.uuid.clone()),
            order_state: billing.order.as_ref().and_then(|order| order.state.clone()),
            order_status: billing
                .order
                .as_ref()
                .and_then(|order| order.status.clone()),
            job_status: billing
                .prediction
                .as_ref()
                .and_then(|prediction| prediction.status.clone()),
        })
        .expect("actual usage metadata serializes"),
    })
}

pub(super) async fn record_usage_estimate(
    state: &AppState,
    task_id: TaskId,
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
    task_id: TaskId,
    prediction: &Prediction,
) -> anyhow::Result<bool> {
    if state
        .durable
        .has_actual_usage(task_id, &prediction.id)
        .await?
    {
        state
            .tasks
            .webhooks(veoveo_types::ExtensionName::parse("media")?)
            .release_retention_after_billing(task_id)
            .await?;
        return Ok(true);
    }
    let task = state
        .tasks
        .get(task_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("media billing task {task_id} not found"))?;
    let job = state
        .durable
        .provider_job_for_task_prediction(task_id, &prediction.id)
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
            .notify_resource_updated(MediaTaskUsageUri::new(task_id)?.to_string())
            .await;
    }
    let complete = recorded > 0
        || state
            .durable
            .has_actual_usage(task_id, &prediction.id)
            .await?;
    if complete {
        state
            .tasks
            .webhooks(veoveo_types::ExtensionName::parse("media")?)
            .release_retention_after_billing(task_id)
            .await?;
    }
    Ok(complete)
}

pub(super) fn spawn_actual_usage_reconciliation(
    state: Arc<AppState>,
    task_id: TaskId,
    prediction: Prediction,
) {
    tokio::spawn(async move {
        let mut delay = Duration::ZERO;
        loop {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            match reconcile_actual_usage_once(&state, task_id, &prediction).await {
                Ok(true) => break,
                Ok(false) => tracing::info!(
                    task_id = %task_id,
                    provider_job_id = %prediction.id,
                    "actual billing usage is not available yet"
                ),
                Err(error) => tracing::warn!(
                    task_id = %task_id,
                    provider_job_id = %prediction.id,
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
    let mut after = None;
    loop {
        let page = match state.durable.billing_candidates(after).await {
            Ok(page) => page,
            Err(error) => {
                tracing::warn!("failed to select pending billing usage: {error}");
                return;
            }
        };
        for job in page.jobs {
            spawn_actual_usage_reconciliation(state.clone(), job.task_id, job.prediction);
        }
        match page.next_job_id {
            Some(next) => after = Some(next),
            None => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use surrealdb::types::SurrealValue;

    #[test]
    fn actual_usage_producers_emit_closed_current_metadata() {
        let task = TaskId::new();
        let prediction: Prediction = serde_json::from_value(
            json!({"id":"usage-job","model":"owner/model","status":"completed"}),
        )
        .unwrap();
        let model: ModelEntry = serde_json::from_value(
            json!({"model_id":"owner/model","type":"text-to-image","base_price":0.5}),
        )
        .unwrap();
        let billing: BillingRecord = serde_json::from_value(json!({"uuid":"billing-1","billing_type":"deduct","price":0.25,"order":{"uuid":"order-1","state":"upstream-state","status":"upstream-status"},"prediction":{"model_uuid":"provider-model","status":"completed"}})).unwrap();
        let schema = serde_json::to_value(schemars::schema_for!(MediaUsageMetadata)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for (usage, fields) in [
            (
                usage_estimate(task, &prediction.id, &model),
                vec![("modelType", "model_type"), ("costKind", "cost_kind")],
            ),
            (
                actual_usage_record(task, &prediction, &billing).unwrap(),
                vec![
                    ("billingType", "billing_type"),
                    ("sourceCreatedAt", "source_created_at"),
                    ("sourceUpdatedAt", "source_updated_at"),
                    ("orderId", "order_id"),
                    ("orderState", "order_state"),
                    ("orderStatus", "order_status"),
                    ("jobStatus", "job_status"),
                ],
            ),
        ] {
            assert!(validator.is_valid(&usage.metadata));
            let typed = MediaUsageMetadata::from_value(
                veoveo_platform_store::native_json_into_value(usage.metadata.clone()),
            )
            .unwrap();
            assert_eq!(serde_json::to_value(typed).unwrap(), usage.metadata);
            for (current, retired) in fields {
                for mode in ["replacement", "mixed", "conflicting"] {
                    let mut bad = usage.metadata.clone();
                    let object = bad.as_object_mut().unwrap();
                    let value = object.get(current).cloned().unwrap();
                    if mode == "replacement" {
                        object.remove(current);
                    }
                    object.insert(
                        retired.into(),
                        if mode == "conflicting" {
                            json!("retired-conflict")
                        } else {
                            value
                        },
                    );
                    assert!(!validator.is_valid(&bad));
                    assert!(
                        MediaUsageMetadata::from_value(
                            veoveo_platform_store::native_json_into_value(bad)
                        )
                        .is_err()
                    );
                }
            }
            for bad in [
                json!({}),
                json!({"source":"unknown_source"}),
                json!({"source":"model_regsitry"}),
            ] {
                assert!(
                    MediaUsageMetadata::from_value(veoveo_platform_store::native_json_into_value(
                        bad
                    ))
                    .is_err()
                );
            }
        }
    }
}
