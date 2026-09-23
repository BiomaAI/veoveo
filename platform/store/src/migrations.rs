use std::collections::BTreeMap;
use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{MigrationError, PlatformStore, StoreError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub filename: &'static str,
    pub sql: &'static str,
}

impl Migration {
    pub fn checksum(&self) -> String {
        let digest = Sha256::digest(self.sql.as_bytes());
        let mut encoded = String::with_capacity(digest.len() * 2);
        for byte in digest {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
        }
        encoded
    }
}

mod catalog;
mod downstream;
use catalog::MIGRATIONS;
pub use downstream::{DownstreamMigration, DownstreamMigrationError, DownstreamSchemaStatus};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AppliedMigration {
    pub id: RecordId,
    pub version: i64,
    pub name: String,
    pub checksum: String,
    pub applied_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaStatus {
    pub current_version: Option<u32>,
    pub latest_version: u32,
    pub pending_versions: Vec<u32>,
    pub downstream: DownstreamSchemaStatus,
}

impl SchemaStatus {
    pub fn is_current(&self) -> bool {
        self.pending_versions.is_empty()
            && self.current_version == Some(self.latest_version)
            && self.downstream.is_current()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationReport {
    pub applied_versions: Vec<u32>,
    pub downstream_applied_versions: Vec<u32>,
    pub status: SchemaStatus,
}

pub fn migrations() -> &'static [Migration] {
    &MIGRATIONS
}

pub fn schema_sql() -> String {
    MIGRATIONS
        .iter()
        .map(|migration| migration.sql)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn validate_catalog(catalog: &[Migration]) -> Result<(), MigrationError> {
    if catalog.is_empty() {
        return Err(MigrationError::EmptyCatalog);
    }
    for (expected, migration) in catalog.iter().enumerate() {
        let expected = expected as u32;
        if migration.version != expected {
            return Err(MigrationError::NonContiguous {
                expected,
                actual: migration.version,
            });
        }
        if migration.name.trim().is_empty() || migration.sql.trim().is_empty() {
            return Err(MigrationError::EmptyMigration {
                version: migration.version,
            });
        }
        let expected_prefix = format!("{:04}_", migration.version);
        if !migration.filename.starts_with(&expected_prefix)
            || !migration.filename.ends_with(".surql")
        {
            return Err(MigrationError::Drift {
                version: migration.version,
            });
        }
    }
    Ok(())
}

fn validate_history(applied: &[AppliedMigration]) -> Result<SchemaStatus, MigrationError> {
    validate_history_for(&MIGRATIONS, applied)
}

fn validate_history_for(
    catalog: &[Migration],
    applied: &[AppliedMigration],
) -> Result<SchemaStatus, MigrationError> {
    validate_catalog(catalog)?;
    let by_version: BTreeMap<u32, &AppliedMigration> = applied
        .iter()
        .map(|migration| (migration.version as u32, migration))
        .collect();

    for migration in applied {
        if migration.version < 0 {
            return Err(MigrationError::DatabaseAhead {
                version: migration.version as u32,
            });
        }
        let version = migration.version as u32;
        let Some(expected) = catalog.get(version as usize) else {
            return Err(MigrationError::DatabaseAhead { version });
        };
        if migration.name != expected.name || migration.checksum != expected.checksum() {
            return Err(MigrationError::Drift { version });
        }
    }

    if let Some(highest) = by_version.keys().next_back().copied() {
        for version in 0..=highest {
            if !by_version.contains_key(&version) {
                return Err(MigrationError::HistoryGap { version });
            }
        }
    }

    let current_version = by_version.keys().next_back().copied();
    let pending_versions = catalog
        .iter()
        .filter(|migration| !by_version.contains_key(&migration.version))
        .map(|migration| migration.version)
        .collect();
    Ok(SchemaStatus {
        current_version,
        latest_version: catalog.last().expect("catalog is non-empty").version,
        pending_versions,
        downstream: DownstreamSchemaStatus::default(),
    })
}

impl PlatformStore {
    async fn migration_history(&self) -> Result<Vec<AppliedMigration>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM platform_schema_migration ORDER BY version ASC;")
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn schema_status(&self) -> Result<SchemaStatus, StoreError> {
        let applied = self.migration_history().await?;
        let mut status = validate_history(&applied)?;
        downstream::validate(downstream::MIGRATIONS, status.latest_version)?;
        status.downstream = downstream::history_status(
            downstream::MIGRATIONS,
            &self
                .downstream_history(
                    status
                        .current_version
                        .is_none_or(|version| version < downstream::HISTORY_UPSTREAM_VERSION),
                )
                .await?,
            status.current_version,
        )?;
        Ok(status)
    }

    pub async fn migrate(&self) -> Result<MigrationReport, StoreError> {
        self.migrate_catalogs(&MIGRATIONS, downstream::MIGRATIONS)
            .await
    }

    async fn migrate_catalogs(
        &self,
        upstream: &[Migration],
        fork: &[DownstreamMigration],
    ) -> Result<MigrationReport, StoreError> {
        self.require_root("schema migration")?;
        validate_catalog(upstream)?;
        let latest = upstream.last().expect("validated catalog").version;
        downstream::validate(fork, latest)?;

        self.db.query(upstream[0].sql).await?.check()?;
        let mut history = self.migration_history().await?;
        let status = validate_history_for(upstream, &history)?;
        downstream::history_status(
            fork,
            &self
                .downstream_history(
                    status
                        .current_version
                        .is_none_or(|version| version < downstream::HISTORY_UPSTREAM_VERSION),
                )
                .await?,
            status.current_version,
        )?;
        let mut applied_versions = Vec::new();

        for version in status.pending_versions.clone() {
            let migration = &upstream[version as usize];
            self.prepare_migration(version).await?;
            let statement = format!(
                "BEGIN TRANSACTION;\n{}\nCREATE platform_schema_migration:{} CONTENT {{ version: {}, name: $migration_name, checksum: $migration_checksum, applied_at: time::now() }};\nCOMMIT TRANSACTION;",
                migration.sql, migration.version, migration.version
            );
            let result = self
                .db
                .query(statement)
                .bind(("migration_name", migration.name))
                .bind(("migration_checksum", migration.checksum()))
                .await
                .map_err(|error| (0, error))
                .and_then(|mut response| {
                    match crate::store::primary_transaction_failure(response.take_errors()) {
                        Some(error) => Err(error),
                        None => Ok(()),
                    }
                });

            if let Err((statement, error)) = result {
                // A second replica may have committed the same migration first.
                // Only accept that race when the durable checksum matches exactly.
                history = self.migration_history().await?;
                match validate_history_for(upstream, &history) {
                    Ok(current) if !current.pending_versions.contains(&version) => {
                        continue;
                    }
                    _ => {
                        return Err(StoreError::MigrationExecution {
                            version: migration.version,
                            statement,
                            name: migration.name,
                            source: Box::new(error),
                        });
                    }
                }
            }
            applied_versions.push(version);
        }

        history = self.migration_history().await?;
        let mut status = validate_history_for(upstream, &history)?;
        let downstream_applied_versions = self.migrate_downstream(fork, latest).await?;
        status.downstream = downstream::history_status(
            fork,
            &self
                .downstream_history(
                    status
                        .current_version
                        .is_none_or(|version| version < downstream::HISTORY_UPSTREAM_VERSION),
                )
                .await?,
            status.current_version,
        )?;
        Ok(MigrationReport {
            applied_versions,
            downstream_applied_versions,
            status,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::PlatformTable;

    const RELATIONS: [&str; 8] = [
        "membership",
        "artifact_grant",
        "profile_server",
        "task_produced_artifact",
        "task_used_artifact",
        "artifact_derived_from",
        "task_used_frame",
        "agent_owner",
    ];

    #[test]
    fn catalog_is_numbered_and_checksums_are_unique() {
        validate_catalog(migrations()).unwrap();
        let checksums: HashSet<_> = migrations().iter().map(Migration::checksum).collect();
        assert_eq!(checksums.len(), migrations().len());
        assert!(checksums.iter().all(|checksum| checksum.len() == 64));
    }

    #[test]
    fn every_public_table_is_schemafull_with_a_changefeed() {
        let sql = schema_sql();
        assert!(!sql.contains("SCHEMALESS"));
        for table in PlatformTable::ALL {
            let declaration = format!("DEFINE TABLE IF NOT EXISTS {} SCHEMAFULL", table.as_str());
            assert!(sql.contains(&declaration), "missing {declaration}");
            let definition = sql
                .split(&declaration)
                .nth(1)
                .and_then(|tail| tail.split(';').next())
                .unwrap();
            assert!(
                definition.contains("CHANGEFEED"),
                "{} has no changefeed",
                table
            );
            assert!(
                definition.contains("PERMISSIONS NONE"),
                "{} is not private",
                table
            );
        }
    }

    #[test]
    fn graph_relations_and_operational_indexes_are_present() {
        let sql = schema_sql();
        for relation in RELATIONS {
            assert!(sql.contains(&format!(
                "DEFINE TABLE IF NOT EXISTS {relation} SCHEMAFULL TYPE RELATION"
            )));
        }
        for index in [
            "principal_external_identity_unique",
            "artifact_blob_tenant_sha_unique",
            "share_link_token_hash_unique",
            "provider_event_unique",
            "task_status_lease",
            "outbox_event_sequence_unique",
            "artifact_occurrence_search",
            "task_queued_count",
            "uav_control_grant_context_id_unique",
            "uav_mission_plan_context_id_unique",
            "uav_command_lease_vehicle_unique",
        ] {
            assert!(sql.contains(&format!("DEFINE INDEX IF NOT EXISTS {index}")));
        }
    }

    #[test]
    fn unused_audit_full_text_index_is_removed() {
        assert!(
            schema_sql().contains("REMOVE INDEX IF EXISTS audit_event_search ON TABLE audit_event")
        );
    }

    #[test]
    fn task_route_migration_preserves_opaque_protocol_ids() {
        let migration = migrations()
            .iter()
            .find(|migration| migration.version == 41)
            .expect("task route source migration");
        assert!(migration.sql.contains("string::is_uuid(source_task_id)"));
        assert!(migration.sql.contains("option<record<task>>"));
        assert!(
            !migration
                .sql
                .contains("REMOVE FIELD IF EXISTS source_task_id")
        );
    }

    #[test]
    fn optimization_task_identity_indexes_are_declared() {
        let migration = migrations()
            .iter()
            .find(|migration| migration.version == 42)
            .expect("optimization task identity migration");
        for index in [
            "task_domain_owner_page",
            "optimization_task_problem",
            "optimization_task_run",
            "optimization_task_solution",
        ] {
            assert!(migration.sql.contains(index), "missing {index}");
        }
    }

    #[test]
    fn time_acquisition_release_index_is_declared() {
        let migration = migrations()
            .iter()
            .find(|migration| migration.version == 43)
            .expect("time acquisition release migration");
        assert!(migration.sql.contains("time_acquisition_staged_release"));
        assert!(migration.sql.contains("tenant, staged_release_key"));
    }

    #[test]
    fn operation_audit_migration_distinguishes_success_from_policy_allow() {
        let migration = migrations()
            .iter()
            .find(|migration| migration.version == 44)
            .expect("operation audit migration");
        assert!(migration.sql.contains("\"succeeded\""));
        assert!(migration.sql.contains("DEFINE FIELD OVERWRITE outcome"));
    }

    #[test]
    fn history_detects_drift_and_gaps() {
        let applied = AppliedMigration {
            id: RecordId::new("platform_schema_migration", 0_i64),
            version: 0,
            name: MIGRATIONS[0].name.to_owned(),
            checksum: "wrong".to_owned(),
            applied_at: Utc::now(),
        };
        assert_eq!(
            validate_history(&[applied]).unwrap_err(),
            MigrationError::Drift { version: 0 }
        );
    }
}
