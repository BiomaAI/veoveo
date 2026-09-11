//! Online preparation for published migrations whose backfill must commit in batches.
use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::SurrealValue;

use crate::{PlatformStore, StoreError};

const AUDIT_INDEX: &str = "audit_event_resource_time";
const AUDIT_DEFINITION: &str =
    "DEFINE INDEX audit_event_resource_time ON audit_event FIELDS resource_type, occurred_at";
const WAIT_BUDGET: Duration = Duration::from_secs(15 * 60);
const QUERY_BUDGET: Duration = Duration::from_secs(10);

#[derive(Deserialize, SurrealValue)]
struct TableInfo {
    indexes: BTreeMap<String, String>,
}

#[derive(Deserialize, SurrealValue)]
struct IndexInfo {
    building: BuildInfo,
}

#[derive(Deserialize, SurrealValue)]
struct BuildInfo {
    status: BuildStatus,
}

#[derive(Debug, Deserialize, Serialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
enum BuildStatus {
    #[surreal(value = "started")]
    Started,
    #[surreal(value = "cleaning")]
    Cleaning,
    #[surreal(value = "indexing")]
    Indexing,
    #[surreal(value = "ready")]
    Ready,
    #[surreal(value = "aborted")]
    Aborted,
    #[surreal(value = "error")]
    Error,
}

fn rejected(reason: &'static str) -> StoreError {
    StoreError::MigrationPreparation {
        version: 72,
        reason,
    }
}

impl PlatformStore {
    pub(crate) async fn prepare_migration(&self, version: u32) -> Result<(), StoreError> {
        if version != 72 {
            return Ok(());
        }
        // Keep the published SQL and checksum intact. The original IF NOT EXISTS
        // becomes a no-op only after the identical physical index is online.
        tokio::time::timeout(WAIT_BUDGET, self.prepare_audit_index())
            .await
            .map_err(|_| rejected("index readiness deadline exceeded; resume migration to observe the existing build"))?
    }

    async fn prepare_audit_index(&self) -> Result<(), StoreError> {
        self.validate_audit_index(false).await?;
        tokio::time::timeout(QUERY_BUDGET, async {
            self.db
                .query("DEFINE INDEX IF NOT EXISTS audit_event_resource_time ON audit_event FIELDS resource_type, occurred_at CONCURRENTLY;")
                .await?
                .check()?;
            Ok::<_, StoreError>(())
        })
        .await
        .map_err(|_| rejected("index admission deadline exceeded; inspect the existing build before retrying"))??;

        loop {
            let info: IndexInfo = tokio::time::timeout(QUERY_BUDGET, async {
                let mut result = self
                    .db
                    .query("INFO FOR INDEX audit_event_resource_time ON audit_event;")
                    .await?
                    .check()?;
                result
                    .take::<Option<IndexInfo>>(0)?
                    .ok_or_else(|| rejected("index observation returned no status"))
            })
            .await
            .map_err(|_| rejected("index observation deadline exceeded"))??;
            match info.building.status {
                BuildStatus::Ready => return self.validate_audit_index(true).await,
                BuildStatus::Aborted | BuildStatus::Error => {
                    return Err(rejected(
                        "index build failed; inspect INFO FOR INDEX before resuming",
                    ));
                }
                BuildStatus::Started | BuildStatus::Cleaning | BuildStatus::Indexing => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn validate_audit_index(&self, required: bool) -> Result<(), StoreError> {
        let info: TableInfo = tokio::time::timeout(QUERY_BUDGET, async {
            let mut result = self
                .db
                .query("INFO FOR TABLE audit_event;")
                .await?
                .check()?;
            result
                .take::<Option<TableInfo>>(0)?
                .ok_or_else(|| rejected("index definition observation returned no table"))
        })
        .await
        .map_err(|_| rejected("index definition observation deadline exceeded"))??;
        match info.indexes.get(AUDIT_INDEX) {
            Some(definition) if definition == AUDIT_DEFINITION => Ok(()),
            None if !required => Ok(()),
            _ => Err(rejected(
                "audit index definition differs from the published migration",
            )),
        }
    }
}
