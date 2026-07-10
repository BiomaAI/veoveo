use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use crate::{
    CoordinateOperationId, CoordinateOperationRecord, FrameId, FrameRecord, OpenObject,
    OutboxDraft, PlatformIdentity, PlatformStore, StoreError, TaskId, TaskRecord, TenantId,
};

const COORDINATE_EVENT_SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug)]
pub struct CoordinateFrameDraft {
    pub identity: PlatformIdentity,
    pub frame_key: String,
    pub display_name: String,
    pub definition: OpenObject,
    pub proj_pipeline: Option<String>,
    pub classification: String,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CoordinateOperationDraft {
    pub identity: PlatformIdentity,
    pub task_id: Option<TaskId>,
    pub operation_key: String,
    pub kind: String,
    pub provenance: OpenObject,
    pub classification: String,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct FrameContent {
    tenant: RecordId,
    owner: RecordId,
    frame_key: String,
    display_name: String,
    definition: OpenObject,
    proj_pipeline: Option<String>,
    classification: String,
    labels: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct CoordinateOperationContent {
    tenant: RecordId,
    owner: RecordId,
    task: Option<RecordId>,
    operation_key: String,
    kind: String,
    provenance: OpenObject,
    classification: String,
    labels: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PlatformStore {
    pub async fn create_coordinate_frame(
        &self,
        mut draft: CoordinateFrameDraft,
    ) -> Result<FrameRecord, StoreError> {
        validate_text("frame_key", &draft.frame_key, 256)?;
        validate_text("display_name", &draft.display_name, 512)?;
        validate_text("classification", &draft.classification, 256)?;
        validate_optional_text("proj_pipeline", draft.proj_pipeline.as_deref(), 8_192)?;
        normalize_labels(&mut draft.labels)?;
        if self
            .coordinate_frame_by_key(draft.identity.tenant_id, &draft.frame_key)
            .await?
            .is_some()
        {
            return Err(StoreError::CoordinateFrameConflict(draft.frame_key));
        }

        let frame_id = FrameId::new();
        let now = Utc::now();
        let content = FrameContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            frame_key: draft.frame_key.clone(),
            display_name: draft.display_name,
            definition: draft.definition,
            proj_pipeline: draft.proj_pipeline,
            classification: draft.classification,
            labels: draft.labels,
            created_at: now,
            updated_at: now,
        };
        let outbox = coordinate_event(
            &draft.identity,
            "frame",
            &draft.frame_key,
            "coordinate.frame.created",
        );
        let result = self
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $frame CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("frame", frame_id.record_id()))
            .bind(("content", content))
            .bind(("outbox", outbox))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if self
                .coordinate_frame_by_key(draft.identity.tenant_id, &draft.frame_key)
                .await?
                .is_some()
            {
                return Err(StoreError::CoordinateFrameConflict(draft.frame_key));
            }
            return Err(error.into());
        }
        self.coordinate_frame_by_key(draft.identity.tenant_id, &draft.frame_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "coordinate frame creation readback",
            })
    }

    pub async fn coordinate_frame_by_key(
        &self,
        tenant_id: TenantId,
        frame_key: &str,
    ) -> Result<Option<FrameRecord>, StoreError> {
        validate_text("frame_key", frame_key, 256)?;
        let mut response = self
            .client()
            .query("SELECT * FROM frame WHERE tenant = $tenant AND frame_key = $frame_key LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("frame_key", frame_key.to_owned()))
            .await?
            .check()?;
        let records: Vec<FrameRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    pub async fn list_coordinate_frames(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<FrameRecord>, StoreError> {
        let mut response = self
            .client()
            .query("SELECT * FROM frame WHERE tenant = $tenant ORDER BY frame_key ASC;")
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn upsert_coordinate_operation(
        &self,
        mut draft: CoordinateOperationDraft,
    ) -> Result<CoordinateOperationRecord, StoreError> {
        let operation_id = coordinate_operation_id(&draft.operation_key)?;
        validate_text("kind", &draft.kind, 128)?;
        validate_text("classification", &draft.classification, 256)?;
        normalize_labels(&mut draft.labels)?;
        if let Some(task_id) = draft.task_id {
            let task = self
                .coordinate_task(task_id)
                .await?
                .ok_or_else(|| StoreError::TaskNotFound(task_id.to_string()))?;
            if task.tenant != draft.identity.tenant_id.record_id()
                || task.owner != draft.identity.principal_id.record_id()
            {
                return Err(StoreError::CoordinateOperationConflict(draft.operation_key));
            }
        }

        let now = Utc::now();
        let content = CoordinateOperationContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            task: draft.task_id.map(TaskId::record_id),
            operation_key: draft.operation_key.clone(),
            kind: draft.kind,
            provenance: draft.provenance,
            classification: draft.classification,
            labels: draft.labels,
            created_at: draft.created_at,
            updated_at: now,
        };
        if let Some(existing) = self
            .coordinate_operation(draft.identity.tenant_id, &draft.operation_key)
            .await?
        {
            validate_existing_operation(&existing, &content)?;
            return Ok(existing);
        }
        let outbox = coordinate_event(
            &draft.identity,
            "coordinate_operation",
            &draft.operation_key,
            "coordinate.operation.recorded",
        );
        let result = self
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $operation CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("operation", operation_id.record_id()))
            .bind(("content", content.clone()))
            .bind(("outbox", outbox))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self
                .coordinate_operation(draft.identity.tenant_id, &draft.operation_key)
                .await?
            {
                validate_existing_operation(&existing, &content)?;
                return Ok(existing);
            }
            return Err(error.into());
        }
        self.coordinate_operation(draft.identity.tenant_id, &draft.operation_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "coordinate operation creation readback",
            })
    }

    pub async fn coordinate_operation(
        &self,
        tenant_id: TenantId,
        operation_key: &str,
    ) -> Result<Option<CoordinateOperationRecord>, StoreError> {
        let operation_id = coordinate_operation_id(operation_key)?;
        let mut response = self
            .client()
            .query("SELECT * FROM ONLY $operation WHERE tenant = $tenant;")
            .bind(("operation", operation_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    async fn coordinate_task(&self, task_id: TaskId) -> Result<Option<TaskRecord>, StoreError> {
        let mut response = self
            .client()
            .query("SELECT * FROM ONLY $task;")
            .bind(("task", task_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}

fn validate_existing_operation(
    existing: &CoordinateOperationRecord,
    expected: &CoordinateOperationContent,
) -> Result<(), StoreError> {
    if existing.tenant == expected.tenant
        && existing.owner == expected.owner
        && existing.task == expected.task
        && existing.operation_key == expected.operation_key
        && existing.kind == expected.kind
        && existing.provenance == expected.provenance
        && existing.classification == expected.classification
        && existing.labels == expected.labels
        && existing.created_at == expected.created_at
    {
        Ok(())
    } else {
        Err(StoreError::CoordinateOperationConflict(
            expected.operation_key.clone(),
        ))
    }
}

fn coordinate_operation_id(value: &str) -> Result<CoordinateOperationId, StoreError> {
    let raw = value
        .strip_prefix("op-")
        .ok_or_else(|| invalid_coordinate("operation_key", "must be `op-` followed by a UUIDv7"))?;
    let uuid = Uuid::parse_str(raw)
        .map_err(|_| invalid_coordinate("operation_key", "must be `op-` followed by a UUIDv7"))?;
    if uuid.get_version_num() != 7 {
        return Err(invalid_coordinate(
            "operation_key",
            "must be `op-` followed by a UUIDv7",
        ));
    }
    Ok(CoordinateOperationId::from_uuid(uuid))
}

fn coordinate_event(
    identity: &PlatformIdentity,
    aggregate_type: &str,
    aggregate_id: &str,
    event_type: &str,
) -> OutboxDraft {
    OutboxDraft::now(
        Some(identity.tenant_id.record_id()),
        aggregate_type,
        aggregate_id,
        event_type,
        COORDINATE_EVENT_SCHEMA_VERSION,
        OpenObject::new(BTreeMap::from([
            ("tenant_key".into(), serde_json::json!(identity.tenant_key)),
            (
                "principal_key".into(),
                serde_json::json!(identity.principal_key),
            ),
        ])),
    )
}

fn normalize_labels(labels: &mut Vec<String>) -> Result<(), StoreError> {
    for label in labels.iter() {
        validate_text("labels", label, 256)?;
    }
    labels.sort();
    labels.dedup();
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
        return Err(invalid_coordinate(
            field,
            "must be non-empty, bounded, and contain no control characters",
        ));
    }
    Ok(())
}

fn invalid_coordinate(field: &'static str, reason: &'static str) -> StoreError {
    StoreError::InvalidCoordinateField { field, reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_operation_identity_requires_uuid_v7() {
        let uuid_v7 = Uuid::now_v7();
        assert_eq!(
            coordinate_operation_id(&format!("op-{uuid_v7}"))
                .unwrap()
                .as_uuid(),
            uuid_v7
        );
        assert!(coordinate_operation_id(&format!("op-{}", Uuid::new_v4())).is_err());
        assert!(coordinate_operation_id("operation-1").is_err());
    }

    #[test]
    fn labels_are_sorted_and_deduplicated() {
        let mut labels = vec!["pii".to_owned(), "cui".to_owned(), "pii".to_owned()];
        normalize_labels(&mut labels).unwrap();
        assert_eq!(labels, ["cui", "pii"]);
    }
}
