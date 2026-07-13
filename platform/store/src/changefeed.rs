use serde::{Deserialize, Serialize};
use surrealdb::method::Stream;
use surrealdb::types::{SurrealValue, Value};

use crate::{PlatformStore, PlatformTable, StoreError};

const MAX_CHANGEFEED_LIMIT: u32 = 10_000;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ChangefeedCursor(i64);

impl ChangefeedCursor {
    pub const fn initial() -> Self {
        Self(0)
    }

    pub const fn from_versionstamp(value: i64) -> Option<Self> {
        if value < 0 { None } else { Some(Self(value)) }
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
}
