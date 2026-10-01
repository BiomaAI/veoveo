//! Identity-only Task change decoding, shared by trusted and public consumers.
use surrealdb::types::{RecordIdKey, SurrealValue, Value};
use veoveo_types::{ServerSlug, TaskId};

use crate::{ChangefeedEntry, StoreError, TaskRecord};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskChange {
    pub task_id: TaskId,
}

impl TaskChange {
    /// Trusted server workers can replay committed Task states. Public listeners
    /// must use `decode` as an ID hint followed by current SQL-authorized reads.
    pub fn snapshot(
        entry: &ChangefeedEntry,
        server: &ServerSlug,
    ) -> Result<Option<TaskRecord>, StoreError> {
        if Self::decode(entry)?.is_none() {
            return Ok(None);
        }
        let ChangefeedEntry::Upsert(row) = entry else {
            return Ok(None);
        };
        if !matches!(row.get("server"), Value::RecordId(id) if id.table.as_str() == "mcp_server" && matches!(&id.key, RecordIdKey::String(key) if key == server.as_str()))
        {
            return Ok(None);
        }
        TaskRecord::from_value(row.clone()).map(Some).map_err(|_| {
            StoreError::InvalidChangefeedEntry {
                reason: "invalid committed Task record",
            }
        })
    }

    pub fn decode(entry: &ChangefeedEntry) -> Result<Option<Self>, StoreError> {
        let Some(record) = entry.record_id().filter(|id| id.table.as_str() == "task") else {
            return Ok(None);
        };
        let RecordIdKey::Uuid(id) = &record.key else {
            return Err(StoreError::InvalidChangefeedEntry {
                reason: "Task change has a non-UUID identity",
            });
        };
        Ok(Some(Self {
            task_id: TaskId::from_uuid(**id),
        }))
    }
}
