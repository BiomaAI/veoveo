//! Bounded audit retention keeps background cleanup off the full-table scan path.
use chrono::{DateTime, Utc};
use serde::Deserialize;
use surrealdb::types::{RecordId, SurrealValue};

use crate::{GatewayAuditKind, PlatformStore, StoreError};

pub const GATEWAY_AUDIT_BATCH_LIMIT: u32 = 1024;

const DELETE_BATCH: &str = "
DELETE (SELECT VALUE id FROM audit_event WITH INDEX audit_event_resource_time
    WHERE resource_type = $resource_type AND occurred_at < $cutoff LIMIT $limit)
    WHERE resource_type = $resource_type AND occurred_at < $cutoff
    RETURN $before.id AS id TIMEOUT 2s;
";

#[derive(Deserialize, SurrealValue)]
struct DeletedAudit {
    id: RecordId,
}

impl PlatformStore {
    /// One finite batch. Concurrent cleanup can reduce the returned count; the
    /// next scheduled pass discovers any remaining expired records again.
    pub async fn delete_gateway_audit_batch_before(
        &self,
        kind: GatewayAuditKind,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let mut response = self
            .db
            .query(DELETE_BATCH)
            .bind(("resource_type", kind.resource_type()))
            .bind(("cutoff", cutoff))
            .bind(("limit", GATEWAY_AUDIT_BATCH_LIMIT))
            .await?
            .check()?;
        let deleted: Vec<DeletedAudit> = response.take(0)?;
        if deleted.len() > GATEWAY_AUDIT_BATCH_LIMIT as usize
            || deleted
                .iter()
                .any(|row| row.id.table.as_str() != "audit_event")
        {
            return Err(StoreError::MissingRecord {
                operation: "bounded gateway audit retention result",
            });
        }
        Ok(deleted.len() as u64)
    }
}
