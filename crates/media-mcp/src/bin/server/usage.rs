use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use veoveo_mcp_contract::{UsageKind, UsageRecord, now_utc};
use veoveo_media_mcp::{
    provider::{BillingRecord, ModelEntry, Prediction},
    uris,
};

use super::{AppState, BILLING_RECONCILE_INITIAL_DELAY, BILLING_RECONCILE_MAX_DELAY};

fn usage_estimate(task_id: &str, provider_job_id: &str, entry: &ModelEntry) -> UsageRecord {
    #[derive(serde::Serialize)]
    struct EstimateUsageMetadata<'a> {
        source: &'static str,
        model_type: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        formula: Option<&'a str>,
        cost_kind: &'static str,
    }

    UsageRecord {
        task_id: task_id.to_string(),
        source_id: None,
        provider_job_id: Some(provider_job_id.to_string()),
        model_id: entry.model_id.clone(),
        kind: UsageKind::Estimate,
        quantity: Some(1.0),
        unit: Some("run".to_string()),
        amount: entry.base_price,
        currency: entry.base_price.map(|_| "USD".to_string()),
        recorded_at: now_utc(),
        metadata: serde_json::to_value(EstimateUsageMetadata {
            source: "model_registry",
            model_type: entry.model_type.as_str(),
            formula: entry.formula.as_deref(),
            cost_kind: "estimate",
        })
        .expect("estimate usage metadata serializes"),
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
        #[serde(skip_serializing_if = "Option::is_none")]
        source_created_at: Option<DateTime<Utc>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_updated_at: Option<DateTime<Utc>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        order_id: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        order_state: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        order_status: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        job_status: Option<&'a str>,
    }

    let amount = billing.signed_amount()?;
    Some(UsageRecord {
        task_id: task_id.to_string(),
        source_id: Some(billing.uuid.clone()),
        provider_job_id: Some(prediction.id.clone()),
        model_id: billing
            .prediction
            .as_ref()
            .and_then(|p| p.model_uuid.clone())
            .unwrap_or_else(|| prediction.model.clone()),
        kind: UsageKind::Actual,
        quantity: Some(1.0),
        unit: Some("billing_record".to_string()),
        amount: Some(amount),
        currency: Some("USD".to_string()),
        recorded_at: now_utc(),
        metadata: serde_json::to_value(ActualUsageMetadata {
            source: "billing_record",
            billing_type: billing.billing_type.as_str(),
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
                .and_then(|p| p.status.as_deref()),
        })
        .expect("actual usage metadata serializes"),
    })
}

pub(super) fn record_usage_estimate(
    state: &AppState,
    task_id: &str,
    provider_job_id: &str,
    entry: &ModelEntry,
) {
    let record = usage_estimate(task_id, provider_job_id, entry);
    if let Err(e) = state.durable.record_usage(&record) {
        tracing::warn!(
            task_id,
            provider_job_id,
            "failed to persist usage estimate: {e}"
        );
    }
}

async fn reconcile_actual_usage_once(
    state: &AppState,
    task_id: &str,
    prediction: &Prediction,
) -> anyhow::Result<bool> {
    if state.durable.has_actual_usage(task_id, &prediction.id)? {
        return Ok(true);
    }

    let billing_records = state.provider.billing_records(&prediction.id).await?;
    let mut recorded = 0usize;
    for billing in billing_records {
        let Some(record) = actual_usage_record(task_id, prediction, &billing) else {
            tracing::warn!(
                task_id,
                provider_job_id = prediction.id.as_str(),
                billing_id = billing.uuid,
                billing_type = billing.billing_type.as_str(),
                "provider billing row has no supported billable amount"
            );
            continue;
        };
        state.durable.record_usage(&record)?;
        recorded += 1;
    }

    if recorded > 0 {
        state
            .subscribers
            .notify_resource_updated(uris::usage_task_uri(task_id))
            .await;
    }
    Ok(recorded > 0 || state.durable.has_actual_usage(task_id, &prediction.id)?)
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
                Ok(true) => {
                    tracing::info!(
                        task_id,
                        provider_job_id = prediction.id.as_str(),
                        "actual usage recorded"
                    );
                    break;
                }
                Ok(false) => {
                    tracing::info!(
                        task_id,
                        provider_job_id = prediction.id.as_str(),
                        "actual usage not available yet"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        task_id,
                        provider_job_id = prediction.id.as_str(),
                        "actual usage reconciliation failed: {e}"
                    );
                }
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
    let predictions: Vec<Prediction> = state
        .predictions
        .read()
        .await
        .values()
        .filter(|prediction| prediction.status == "completed")
        .cloned()
        .collect();

    for prediction in predictions {
        let task_id = match state.durable.task_id_for_provider_job_id(&prediction.id) {
            Ok(Some(task_id)) => task_id,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(
                    provider_job_id = prediction.id,
                    "failed to find task for actual usage reconciliation: {e}"
                );
                continue;
            }
        };
        match state.durable.has_actual_usage(&task_id, &prediction.id) {
            Ok(true) => {}
            Ok(false) => spawn_actual_usage_reconciliation(state.clone(), task_id, prediction),
            Err(e) => tracing::warn!(
                task_id,
                provider_job_id = prediction.id,
                "failed to check actual usage state: {e}"
            ),
        }
    }
}
