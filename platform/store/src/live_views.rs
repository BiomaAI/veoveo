use std::collections::BTreeMap;

use crate::{AuditEventRecord, OpenObject, OutboxDraft, PlatformStore, StoreError};

impl PlatformStore {
    /// Append one simulator-hosted live-view access event and its outbox projection
    /// atomically. Live-view configuration remains simulator-owned; only the access
    /// audit is durable platform state.
    pub async fn append_live_view_audit(&self, record: AuditEventRecord) -> Result<(), StoreError> {
        let outbox = OutboxDraft::now(
            record.tenant.clone(),
            "live_view",
            record.resource_id.clone().unwrap_or_default(),
            format!("live_view.{}", record.action),
            1,
            OpenObject::new(BTreeMap::from([
                ("action".into(), serde_json::json!(&record.action)),
                ("outcome".into(), serde_json::json!(&record.outcome)),
                ("occurred_at".into(), serde_json::json!(record.occurred_at)),
            ])),
        );
        self.db
            .query("BEGIN TRANSACTION; CREATE ONLY $record CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("record", record.id.clone()))
            .bind(("content", record))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }
}
