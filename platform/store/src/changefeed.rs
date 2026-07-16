use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::method::Stream;
use surrealdb::types::{RecordId, SurrealValue, Value};

use crate::{PlatformStore, PlatformTable, StoreError};

const MAX_CHANGEFEED_LIMIT: u32 = 10_000;

/// SurrealDB single-node versionstamps are `unix_millis << 16 | logical`.
/// `SHOW CHANGES … SINCE d'<datetime>'` returns nothing on this deployment
/// (pinned by `changefeed_replay_contract_is_pinned`), so wall-clock anchors
/// are converted to versionstamps through this layout instead. The same
/// integration test pins the layout; a SurrealDB upgrade that changes it
/// fails there, not in production.
const ORACLE_VERSIONSTAMP_SHIFT: u32 = 16;

/// Margin subtracted when anchoring a cursor to a clock reading, covering
/// in-flight time between reading the clock and the writes it must not miss.
/// Over-replay is safe: consumers treat deliveries as idempotent upserts.
const CLOCK_ANCHOR_MARGIN_MS: i64 = 2_000;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ChangefeedCursor(i64);

impl ChangefeedCursor {
    pub const fn initial() -> Self {
        Self(0)
    }

    pub const fn from_versionstamp(value: i64) -> Option<Self> {
        if value < 0 { None } else { Some(Self(value)) }
    }

    /// Anchor a cursor at a wall-clock instant using the pinned oracle
    /// layout, including the safety margin. The instant should come from the
    /// database's own clock (see [`PlatformStore::changefeed_cursor_now`]) so
    /// client clock skew cannot open a replay gap.
    pub fn from_instant(instant: DateTime<Utc>) -> Self {
        let millis = (instant.timestamp_millis() - CLOCK_ANCHOR_MARGIN_MS).max(0);
        Self(millis << ORACLE_VERSIONSTAMP_SHIFT)
    }

    pub const fn versionstamp(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ChangefeedBatch {
    pub versionstamp: i64,
    pub changes: Vec<Value>,
}

/// One decoded change inside a [`ChangefeedBatch`].
///
/// Tables are defined `CHANGEFEED … INCLUDE ORIGINAL`, so `Upsert` carries
/// the full record value after the mutation and `Delete` carries the record
/// id plus the full record as it existed before deletion — consumers can
/// tenant-filter deletes by the original's content. `Definition` covers
/// schema entries such as `define_table` and carries no row data.
#[derive(Clone, Debug, PartialEq)]
pub enum ChangefeedEntry {
    Upsert(Value),
    Delete {
        record: RecordId,
        original: Option<Value>,
    },
    Definition,
}

pub fn decode_changefeed_entry(change: &Value) -> Result<ChangefeedEntry, StoreError> {
    if !change.get("define_table").is_nullish() {
        return Ok(ChangefeedEntry::Definition);
    }
    let delete = change.get("delete");
    if !delete.is_nullish() {
        let record = match delete {
            Value::RecordId(record) => Some(record.clone()),
            Value::Object(_) => match delete.get("id") {
                Value::RecordId(record) => Some(record.clone()),
                _ => None,
            },
            _ => None,
        };
        let original = match delete.get("original") {
            original @ Value::Object(_) => Some(original.clone()),
            _ => None,
        };
        return record
            .map(|record| ChangefeedEntry::Delete { record, original })
            .ok_or(StoreError::InvalidChangefeedEntry {
                reason: "delete entry carried no record id",
            });
    }
    // INCLUDE ORIGINAL updates arrive as `{current: <row>, update: [patches]}`;
    // creates arrive as `{update: <row>}`. Check `current` first — an update's
    // `update` key holds patches, not the row.
    let current = change.get("current");
    if matches!(current, Value::Object(_)) {
        return Ok(ChangefeedEntry::Upsert(current.clone()));
    }
    for key in ["create", "update"] {
        let value = change.get(key);
        if matches!(value, Value::Object(_)) {
            return Ok(ChangefeedEntry::Upsert(value.clone()));
        }
    }
    Err(StoreError::InvalidChangefeedEntry {
        reason: "unrecognized changefeed entry shape",
    })
}

pub type LiveStream<T> = Stream<Vec<T>>;

impl PlatformStore {
    /// Subscribe to future changes. Consumers must replay the table changefeed
    /// from their durable cursor before treating LIVE delivery as current.
    pub async fn live<T>(&self, table: PlatformTable) -> Result<LiveStream<T>, StoreError>
    where
        T: SurrealValue + Unpin,
    {
        Ok(self.db.select(table.as_str()).live().await?)
    }

    /// A cursor anchored "now" on the database's own clock. Capture it before
    /// reading a projection so replay from the cursor overlaps the projection
    /// instead of gapping it.
    pub async fn changefeed_cursor_now(&self) -> Result<ChangefeedCursor, StoreError> {
        let mut response = self.db.query("RETURN time::now();").await?.check()?;
        let now: Value = response.take(0)?;
        let Value::Datetime(now) = now else {
            return Err(StoreError::MissingRecord {
                operation: "changefeed_cursor_now",
            });
        };
        Ok(ChangefeedCursor::from_instant(now.into_inner()))
    }

    /// Replay committed changes for `table` after `cursor`.
    ///
    /// Replay may redeliver the batch at the cursor itself; consumers must
    /// treat deliveries as idempotent upserts.
    pub async fn replay_changes(
        &self,
        table: PlatformTable,
        cursor: ChangefeedCursor,
        limit: u32,
    ) -> Result<Vec<ChangefeedBatch>, StoreError> {
        if limit == 0 || limit > MAX_CHANGEFEED_LIMIT {
            return Err(StoreError::InvalidChangefeedLimit {
                max: MAX_CHANGEFEED_LIMIT,
            });
        }
        let statement = format!(
            "SHOW CHANGES FOR TABLE {} SINCE {} LIMIT {};",
            table.as_str(),
            cursor.versionstamp(),
            limit
        );
        let mut response = self.db.query(statement).await?.check()?;
        Ok(response.take(0)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_rejects_negative_versionstamps() {
        assert_eq!(ChangefeedCursor::from_versionstamp(-1), None);
        assert_eq!(ChangefeedCursor::initial().versionstamp(), 0);
    }

    #[test]
    fn instant_anchors_use_the_pinned_oracle_layout() {
        let instant = DateTime::from_timestamp_millis(1_784_175_969_370).unwrap();
        let cursor = ChangefeedCursor::from_instant(instant);
        assert_eq!(
            cursor.versionstamp(),
            (1_784_175_969_370 - CLOCK_ANCHOR_MARGIN_MS) << ORACLE_VERSIONSTAMP_SHIFT
        );
        let epoch = ChangefeedCursor::from_instant(DateTime::from_timestamp_millis(0).unwrap());
        assert_eq!(epoch.versionstamp(), 0, "pre-epoch anchors clamp to zero");
    }

    #[test]
    fn decoder_reads_define_table_entries() {
        let change = Value::from_t(serde_json::json!({"define_table": {"name": "task"}}));
        assert_eq!(
            decode_changefeed_entry(&change).expect("definition decodes"),
            ChangefeedEntry::Definition
        );
    }

    #[test]
    fn decoder_rejects_unknown_shapes() {
        let change = Value::from_t(serde_json::json!({"mystery": true}));
        assert!(matches!(
            decode_changefeed_entry(&change),
            Err(StoreError::InvalidChangefeedEntry { .. })
        ));
    }
}
