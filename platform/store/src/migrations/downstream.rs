use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use thiserror::Error;

use super::{Migration, validate_catalog};
use crate::{MigrationError, PlatformStore, StoreError};

#[path = "../../downstream/catalog.rs"]
mod catalog;
pub(super) use catalog::MIGRATIONS;

/// Upstream step that introduces the downstream history table.
pub(super) const HISTORY_UPSTREAM_VERSION: u32 = 92;

/// One fork-owned migration, ordered after all compiled upstream migrations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DownstreamMigration {
    pub migration: Migration,
    pub requires_upstream: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DownstreamSchemaStatus {
    pub current_version: Option<u32>,
    pub latest_version: Option<u32>,
    pub pending_versions: Vec<u32>,
}
impl DownstreamSchemaStatus {
    pub fn is_current(&self) -> bool {
        self.pending_versions.is_empty() && self.current_version == self.latest_version
    }
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum DownstreamMigrationError {
    #[error("invalid downstream catalog: {0}")]
    Catalog(#[from] MigrationError),
    #[error(
        "downstream migration {version} needs upstream migration {required}, which this \
         revision does not include; merge the upstream change that adds it"
    )]
    MissingUpstream { version: u32, required: u32 },
    #[error(
        "downstream migration {version} depends on an earlier upstream version than the \
         migration before it; each step must depend on the same or a later upstream version"
    )]
    DependencyOrder { version: u32 },
    #[error(
        "the database history for downstream migration {version} is malformed or repeats a \
         version; restore the history from backup before running migrations"
    )]
    InvalidHistory { version: i64 },
    #[error(
        "the database has downstream migration {version}, which this fork revision does not \
         include; deploy the fork revision that added it"
    )]
    DatabaseAhead { version: i64 },
    #[error(
        "downstream migration {version} changed after it was deployed; restore the original \
         migration and add a new one for the correction"
    )]
    Drift { version: u32 },
    #[error("downstream migration history is missing versions before {version}")]
    HistoryGap { version: u32 },
    #[error(
        "downstream migration {version} records an upstream dependency that has not been applied"
    )]
    UnappliedUpstream { version: u32 },
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub(super) struct AppliedDownstreamMigration {
    id: RecordId,
    version: i64,
    name: String,
    filename: String,
    checksum: String,
    requires_upstream: i64,
    applied_at: DateTime<Utc>,
}

pub(super) fn validate(
    catalog: &[DownstreamMigration],
    upstream_latest: u32,
) -> Result<(), DownstreamMigrationError> {
    if catalog.is_empty() {
        return Ok(());
    }
    validate_catalog(&catalog.iter().map(|m| m.migration).collect::<Vec<_>>())?;
    let mut previous = 0;
    for item in catalog {
        if item.requires_upstream > upstream_latest {
            return Err(DownstreamMigrationError::MissingUpstream {
                version: item.migration.version,
                required: item.requires_upstream,
            });
        }
        if item.requires_upstream < previous {
            return Err(DownstreamMigrationError::DependencyOrder {
                version: item.migration.version,
            });
        }
        previous = item.requires_upstream;
    }
    Ok(())
}

pub(super) fn history_status(
    catalog: &[DownstreamMigration],
    applied: &[AppliedDownstreamMigration],
    upstream_current: Option<u32>,
) -> Result<DownstreamSchemaStatus, DownstreamMigrationError> {
    let mut versions = BTreeMap::new();
    for row in applied {
        let version =
            u32::try_from(row.version).map_err(|_| DownstreamMigrationError::InvalidHistory {
                version: row.version,
            })?;
        if row.id != RecordId::new("platform_downstream_migration", row.version)
            || versions.insert(version, row).is_some()
        {
            return Err(DownstreamMigrationError::InvalidHistory {
                version: row.version,
            });
        }
        let Some(expected) = catalog.get(version as usize) else {
            return Err(DownstreamMigrationError::DatabaseAhead {
                version: row.version,
            });
        };
        if row.name != expected.migration.name
            || row.filename != expected.migration.filename
            || row.checksum != expected.migration.checksum()
            || row.requires_upstream != i64::from(expected.requires_upstream)
        {
            return Err(DownstreamMigrationError::Drift { version });
        }
        if upstream_current.is_none_or(|current| current < expected.requires_upstream) {
            return Err(DownstreamMigrationError::UnappliedUpstream { version });
        }
    }
    if let Some(highest) = versions.keys().next_back() {
        for version in 0..=*highest {
            if !versions.contains_key(&version) {
                return Err(DownstreamMigrationError::HistoryGap { version });
            }
        }
    }
    Ok(DownstreamSchemaStatus {
        current_version: versions.keys().next_back().copied(),
        latest_version: catalog.last().map(|m| m.migration.version),
        pending_versions: catalog
            .iter()
            .map(|m| m.migration.version)
            .filter(|version| !versions.contains_key(version))
            .collect(),
    })
}

impl PlatformStore {
    pub(super) async fn downstream_history(
        &self,
        allow_missing_table: bool,
    ) -> Result<Vec<AppliedDownstreamMigration>, StoreError> {
        let result = self
            .db
            .query("SELECT * FROM platform_downstream_migration ORDER BY version ASC;")
            .await?
            .check();
        let mut result = match result {
            Err(error)
                if allow_missing_table
                    && matches!(
                        error.not_found_details(),
                        Some(surrealdb::types::NotFoundError::Table { name }) if name == "platform_downstream_migration"
                    ) =>
            {
                return Ok(Vec::new());
            }
            other => other?,
        };
        Ok(result.take(0)?)
    }

    pub(super) async fn migrate_downstream(
        &self,
        catalog: &[DownstreamMigration],
        upstream_version: u32,
    ) -> Result<Vec<u32>, StoreError> {
        let history = self
            .downstream_history(catalog.is_empty() && upstream_version < HISTORY_UPSTREAM_VERSION)
            .await?;
        let status = history_status(catalog, &history, Some(upstream_version))?;
        let mut applied = Vec::new();
        for version in status.pending_versions {
            let item = &catalog[version as usize];
            let migration = &item.migration;
            let sql = format!(
                "BEGIN TRANSACTION;\n{}\nCREATE platform_downstream_migration:{} CONTENT {{ version: {}, name: $name, filename: $filename, checksum: $checksum, requires_upstream: $requires_upstream, applied_at: time::now() }};\nCOMMIT TRANSACTION;",
                migration.sql, version, version
            );
            let result = self
                .db
                .query(sql)
                .bind(("name", migration.name))
                .bind(("filename", migration.filename))
                .bind(("checksum", migration.checksum()))
                .bind(("requires_upstream", i64::from(item.requires_upstream)))
                .await
                .map_err(|error| (0, error))
                .and_then(|mut response| {
                    match crate::store::primary_transaction_failure(response.take_errors()) {
                        Some(error) => Err(error),
                        None => Ok(()),
                    }
                });
            if let Err((statement, source)) = result {
                let history = self
                    .downstream_history(
                        catalog.is_empty() && upstream_version < HISTORY_UPSTREAM_VERSION,
                    )
                    .await?;
                if history_status(catalog, &history, Some(upstream_version))?
                    .pending_versions
                    .contains(&version)
                {
                    return Err(StoreError::DownstreamMigrationExecution {
                        version,
                        name: migration.name,
                        statement,
                        source: Box::new(source),
                    });
                }
            } else {
                applied.push(version);
            }
        }
        Ok(applied)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod installation_tests;
