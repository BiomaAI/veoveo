//! Recording visibility, cursor pages, completions and layer counts in SQL.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use crate::{PlatformStore, RecordingId, RecordingRecord, StoreError, TenantId};

// Shared by exact reads, collection pages and completions. Recording visibility is
// tenant-wide for callers whose clearance contains every recording label.
const VISIBLE: &str = "tenant = $tenant AND $clearance CONTAINSALL labels";

#[derive(Clone, Debug)]
pub struct RecordingReadScope {
    pub tenant_id: TenantId,
    pub data_labels: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordingCursor {
    pub started_at: DateTime<Utc>,
    pub recording_id: RecordingId,
}

impl RecordingCursor {
    pub fn validate(&self) -> Result<(), StoreError> {
        if self.recording_id.as_uuid().get_version_num() != 7 {
            return Err(StoreError::InvalidRecordingField {
                field: "cursor",
                reason: "recording ID must be UUIDv7",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordingLayerCounts {
    pub total: usize,
    pub committed: usize,
}

impl PlatformStore {
    pub async fn visible_recording(
        &self,
        scope: &RecordingReadScope,
        recording_id: RecordingId,
    ) -> Result<Option<RecordingRecord>, StoreError> {
        let mut response = self
            .db
            .query(format!("SELECT * FROM ONLY $recording WHERE {VISIBLE};"))
            .bind(("recording", recording_id.record_id()))
            .bind(("tenant", scope.tenant_id.record_id()))
            .bind(("clearance", scope.data_labels.clone()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn list_recordings(
        &self,
        scope: &RecordingReadScope,
        after: Option<&RecordingCursor>,
        limit: u32,
    ) -> Result<Vec<RecordingRecord>, StoreError> {
        validate_limit(limit)?;
        if let Some(after) = after {
            after.validate()?;
        }
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM recording WHERE {VISIBLE}
             AND ($after_id = NONE OR started_at < $after_start
                 OR (started_at = $after_start AND id < $after_id))
             ORDER BY started_at DESC, id DESC LIMIT $limit;"
            ))
            .bind(("tenant", scope.tenant_id.record_id()))
            .bind(("clearance", scope.data_labels.clone()))
            .bind(("after_start", after.map(|cursor| cursor.started_at)))
            .bind((
                "after_id",
                after.map(|cursor| cursor.recording_id.record_id()),
            ))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn complete_recording_ids(
        &self,
        scope: &RecordingReadScope,
        needle: &str,
        limit: u32,
    ) -> Result<Vec<String>, StoreError> {
        validate_limit(limit)?;
        if needle.len() > 512 || needle.chars().any(char::is_control) {
            return Err(StoreError::InvalidRecordingField {
                field: "completion",
                reason: "must be at most 512 bytes without control characters",
            });
        }
        #[derive(Deserialize, SurrealValue)]
        struct Completion {
            value: String,
        }
        let mut response = self
            .db
            .query(format!(
                "SELECT type::string(record::id(id)) AS value FROM recording
             WHERE {VISIBLE}
             AND (string::contains(string::lowercase(type::string(record::id(id))), $needle)
                  OR string::contains(string::lowercase(recording_key), $needle))
             ORDER BY value ASC LIMIT $limit;"
            ))
            .bind(("tenant", scope.tenant_id.record_id()))
            .bind(("clearance", scope.data_labels.clone()))
            .bind(("needle", needle.to_lowercase()))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        let values: Vec<Completion> = response.take(0)?;
        Ok(values.into_iter().map(|value| value.value).collect())
    }

    /// Counts catalog layers without materializing their manifests.
    pub async fn recording_layer_counts(
        &self,
        tenant_id: TenantId,
        recording_id: RecordingId,
    ) -> Result<RecordingLayerCounts, StoreError> {
        let mut response = self
            .db
            .query(
                "BEGIN TRANSACTION;
             SELECT VALUE count() FROM recording_layer
             WHERE tenant = $tenant AND recording = $recording GROUP ALL;
             SELECT VALUE count() FROM recording_layer
             WHERE tenant = $tenant AND recording = $recording AND state = 'committed' GROUP ALL;
             COMMIT TRANSACTION;",
            )
            .bind(("tenant", tenant_id.record_id()))
            .bind(("recording", recording_id.record_id()))
            .await?
            .check()?;
        // The SDK includes BEGIN's empty result at statement zero.
        let total: Vec<usize> = response.take(1)?;
        let committed: Vec<usize> = response.take(2)?;
        Ok(RecordingLayerCounts {
            total: total.into_iter().next().unwrap_or(0),
            committed: committed.into_iter().next().unwrap_or(0),
        })
    }
}

fn validate_limit(limit: u32) -> Result<(), StoreError> {
    if !(1..=101).contains(&limit) {
        return Err(StoreError::InvalidRecordingField {
            field: "limit",
            reason: "must be in 1..=101",
        });
    }
    Ok(())
}
