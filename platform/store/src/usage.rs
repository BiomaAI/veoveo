use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use uuid::Uuid;

use crate::{
    DomainUsageId, DomainUsageKind, DomainUsageRecord, OpenObject, OutboxDraft, PlatformStore,
    StoreError, TaskId, TaskRecord,
};

const DOMAIN_USAGE_EVENT_SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct DomainUsageDraft {
    pub task_id: TaskId,
    pub server: String,
    pub source_id: Option<String>,
    pub provider_job_id: Option<String>,
    pub model_id: String,
    pub kind: DomainUsageKind,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub metadata: OpenObject,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Clone, Debug, SurrealValue)]
struct DomainUsageContent {
    tenant: RecordId,
    task: RecordId,
    server: RecordId,
    source_id: Option<String>,
    provider_job_id: Option<String>,
    model_id: String,
    kind: DomainUsageKind,
    quantity: Option<f64>,
    unit: Option<String>,
    amount: Option<f64>,
    currency: Option<String>,
    metadata: OpenObject,
    recorded_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PlatformStore {
    pub async fn upsert_domain_usage(
        &self,
        draft: DomainUsageDraft,
    ) -> Result<DomainUsageRecord, StoreError> {
        validate_usage(&draft)?;
        let task = self
            .task_for_usage(draft.task_id)
            .await?
            .ok_or_else(|| StoreError::TaskNotFound(draft.task_id.to_string()))?;
        let server = RecordId::new("mcp_server", draft.server.clone());
        if task.server != server {
            return Err(StoreError::TaskServerMismatch {
                task_id: draft.task_id.to_string(),
                server: draft.server,
            });
        }
        let usage_id = domain_usage_id(&draft);
        let now = Utc::now();
        let content = DomainUsageContent {
            tenant: task.tenant.clone(),
            task: draft.task_id.record_id(),
            server,
            source_id: draft.source_id.clone(),
            provider_job_id: draft.provider_job_id.clone(),
            model_id: draft.model_id.clone(),
            kind: draft.kind,
            quantity: draft.quantity,
            unit: draft.unit.clone(),
            amount: draft.amount,
            currency: draft.currency.clone(),
            metadata: draft.metadata,
            recorded_at: draft.recorded_at,
            updated_at: now,
        };
        let outbox = OutboxDraft::now(
            Some(task.tenant),
            "domain_usage",
            usage_id.to_string(),
            "domain.usage.recorded",
            DOMAIN_USAGE_EVENT_SCHEMA_VERSION,
            OpenObject::new(BTreeMap::from([
                (
                    "task_id".into(),
                    serde_json::json!(draft.task_id.to_string()),
                ),
                ("server".into(), serde_json::json!(draft.server)),
                ("model_id".into(), serde_json::json!(draft.model_id)),
                (
                    "kind".into(),
                    serde_json::json!(usage_kind_name(draft.kind)),
                ),
            ])),
        );
        self.client()
            .query("BEGIN TRANSACTION; UPSERT ONLY $usage CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("usage", usage_id.record_id()))
            .bind(("content", content))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        let mut response = self
            .client()
            .query("SELECT * FROM ONLY $usage;")
            .bind(("usage", usage_id.record_id()))
            .await?
            .check()?;
        response
            .take::<Option<DomainUsageRecord>>(0)?
            .ok_or(StoreError::MissingRecord {
                operation: "domain usage upsert readback",
            })
    }

    pub async fn domain_usage_for_task(
        &self,
        server: &str,
        task_id: TaskId,
    ) -> Result<Vec<DomainUsageRecord>, StoreError> {
        validate_server(server)?;
        let mut response = self
            .client()
            .query("SELECT * FROM domain_usage WHERE server = $server AND task = $task ORDER BY recorded_at ASC, id ASC;")
            .bind(("server", RecordId::new("mcp_server", server.to_owned())))
            .bind(("task", task_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn domain_usage_task_ids(&self, server: &str) -> Result<Vec<TaskId>, StoreError> {
        validate_server(server)?;
        let mut response = self
            .client()
            .query("SELECT VALUE task FROM domain_usage WHERE server = $server GROUP BY task ORDER BY task ASC;")
            .bind(("server", RecordId::new("mcp_server", server.to_owned())))
            .await?
            .check()?;
        response
            .take::<Vec<RecordId>>(0)?
            .into_iter()
            .map(task_id_from_record)
            .collect()
    }

    async fn task_for_usage(&self, task_id: TaskId) -> Result<Option<TaskRecord>, StoreError> {
        let mut response = self
            .client()
            .query("SELECT * FROM ONLY $task;")
            .bind(("task", task_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}

fn validate_usage(draft: &DomainUsageDraft) -> Result<(), StoreError> {
    validate_server(&draft.server)?;
    validate_text("model_id", &draft.model_id, 256)?;
    validate_optional_text("source_id", draft.source_id.as_deref(), 512)?;
    validate_optional_text("provider_job_id", draft.provider_job_id.as_deref(), 512)?;
    validate_optional_text("unit", draft.unit.as_deref(), 64)?;
    validate_optional_text("currency", draft.currency.as_deref(), 16)?;
    if draft.quantity.is_some_and(|value| !value.is_finite()) {
        return Err(invalid_usage("quantity", "must be finite"));
    }
    if draft.amount.is_some_and(|value| !value.is_finite()) {
        return Err(invalid_usage("amount", "must be finite"));
    }
    Ok(())
}

fn validate_server(value: &str) -> Result<(), StoreError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid_usage(
            "server",
            "must be 1..=128 ASCII letters, digits, hyphens, or underscores",
        ));
    }
    Ok(())
}

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    max: usize,
) -> Result<(), StoreError> {
    if let Some(value) = value {
        validate_text(field, value, max)?;
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), StoreError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid_usage(
            field,
            "must be non-empty, bounded, and contain no control characters",
        ));
    }
    Ok(())
}

fn invalid_usage(field: &'static str, reason: &'static str) -> StoreError {
    StoreError::InvalidUsageField { field, reason }
}

fn domain_usage_id(draft: &DomainUsageDraft) -> DomainUsageId {
    let key = format!(
        "{}|{}|{}|{}|{}|{}",
        draft.server,
        draft.task_id,
        usage_kind_name(draft.kind),
        draft.model_id,
        draft.source_id.as_deref().unwrap_or_default(),
        draft.provider_job_id.as_deref().unwrap_or_default(),
    );
    DomainUsageId::from_uuid(Uuid::new_v5(&Uuid::NAMESPACE_OID, key.as_bytes()))
}

fn usage_kind_name(kind: DomainUsageKind) -> &'static str {
    match kind {
        DomainUsageKind::Estimate => "estimate",
        DomainUsageKind::Actual => "actual",
    }
}

fn task_id_from_record(record: RecordId) -> Result<TaskId, StoreError> {
    if record.table.as_str() != TaskId::TABLE {
        return Err(StoreError::MissingRecord {
            operation: "domain usage task identity",
        });
    }
    match record.key {
        RecordIdKey::Uuid(value) => Ok(TaskId::from_uuid(*value)),
        _ => Err(StoreError::MissingRecord {
            operation: "domain usage task UUID",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> DomainUsageDraft {
        DomainUsageDraft {
            task_id: TaskId::new(),
            server: "optimization".to_owned(),
            source_id: None,
            provider_job_id: None,
            model_id: "optimization/microlp".to_owned(),
            kind: DomainUsageKind::Actual,
            quantity: Some(2.0),
            unit: Some("option".to_owned()),
            amount: None,
            currency: None,
            metadata: OpenObject::default(),
            recorded_at: Utc::now(),
        }
    }

    #[test]
    fn logical_usage_identity_is_stable() {
        let draft = draft();
        assert_eq!(domain_usage_id(&draft), domain_usage_id(&draft));
    }

    #[test]
    fn rejects_non_finite_quantities() {
        let mut draft = draft();
        draft.quantity = Some(f64::NAN);
        assert!(matches!(
            validate_usage(&draft),
            Err(StoreError::InvalidUsageField {
                field: "quantity",
                ..
            })
        ));
    }
}
