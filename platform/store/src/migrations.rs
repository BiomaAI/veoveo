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

const MIGRATIONS: [Migration; 27] = [
    Migration {
        version: 0,
        name: "schema_migrations",
        filename: "0000_schema_migrations.surql",
        sql: include_str!("../migrations/0000_schema_migrations.surql"),
    },
    Migration {
        version: 1,
        name: "identity_control",
        filename: "0001_identity_control.surql",
        sql: include_str!("../migrations/0001_identity_control.surql"),
    },
    Migration {
        version: 2,
        name: "work_artifacts",
        filename: "0002_work_artifacts.surql",
        sql: include_str!("../migrations/0002_work_artifacts.surql"),
    },
    Migration {
        version: 3,
        name: "recordings_agents",
        filename: "0003_recordings_agents.surql",
        sql: include_str!("../migrations/0003_recordings_agents.surql"),
    },
    Migration {
        version: 4,
        name: "graph_edges",
        filename: "0004_graph_edges.surql",
        sql: include_str!("../migrations/0004_graph_edges.surql"),
    },
    Migration {
        version: 5,
        name: "search_and_counts",
        filename: "0005_search_and_counts.surql",
        sql: include_str!("../migrations/0005_search_and_counts.surql"),
    },
    Migration {
        version: 6,
        name: "gateway_control_plane",
        filename: "0006_gateway_control_plane.surql",
        sql: include_str!("../migrations/0006_gateway_control_plane.surql"),
    },
    Migration {
        version: 7,
        name: "artifact_plane",
        filename: "0007_artifact_plane.surql",
        sql: include_str!("../migrations/0007_artifact_plane.surql"),
    },
    Migration {
        version: 8,
        name: "task_runtime",
        filename: "0008_task_runtime.surql",
        sql: include_str!("../migrations/0008_task_runtime.surql"),
    },
    Migration {
        version: 9,
        name: "task_inputs",
        filename: "0009_task_inputs.surql",
        sql: include_str!("../migrations/0009_task_inputs.surql"),
    },
    Migration {
        version: 10,
        name: "agent_runtime",
        filename: "0010_agent_runtime.surql",
        sql: include_str!("../migrations/0010_agent_runtime.surql"),
    },
    Migration {
        version: 11,
        name: "recording_plane",
        filename: "0011_recording_plane.surql",
        sql: include_str!("../migrations/0011_recording_plane.surql"),
    },
    Migration {
        version: 12,
        name: "task_retention_pins",
        filename: "0012_task_retention_pins.surql",
        sql: include_str!("../migrations/0012_task_retention_pins.surql"),
    },
    Migration {
        version: 13,
        name: "gateway_runtime",
        filename: "0013_gateway_runtime.surql",
        sql: include_str!("../migrations/0013_gateway_runtime.surql"),
    },
    Migration {
        version: 14,
        name: "media_artifact_completion",
        filename: "0014_media_artifact_completion.surql",
        sql: include_str!("../migrations/0014_media_artifact_completion.surql"),
    },
    Migration {
        version: 15,
        name: "domain_usage",
        filename: "0015_domain_usage.surql",
        sql: include_str!("../migrations/0015_domain_usage.surql"),
    },
    Migration {
        version: 16,
        name: "gateway_refresh_tokens",
        filename: "0016_gateway_refresh_tokens.surql",
        sql: include_str!("../migrations/0016_gateway_refresh_tokens.surql"),
    },
    Migration {
        version: 17,
        name: "coordinate_operations",
        filename: "0017_coordinate_operations.surql",
        sql: include_str!("../migrations/0017_coordinate_operations.surql"),
    },
    Migration {
        version: 18,
        name: "map_domain",
        filename: "0018_map_domain.surql",
        sql: include_str!("../migrations/0018_map_domain.surql"),
    },
    Migration {
        version: 19,
        name: "time_domain",
        filename: "0019_time_domain.surql",
        sql: include_str!("../migrations/0019_time_domain.surql"),
    },
    Migration {
        version: 20,
        name: "audit_write_path",
        filename: "0020_audit_write_path.surql",
        sql: include_str!("../migrations/0020_audit_write_path.surql"),
    },
    Migration {
        version: 21,
        name: "recording_ingest",
        filename: "0021_recording_ingest.surql",
        sql: include_str!("../migrations/0021_recording_ingest.surql"),
    },
    Migration {
        version: 22,
        name: "recording_ingest_quotas",
        filename: "0022_recording_ingest_quotas.surql",
        sql: include_str!("../migrations/0022_recording_ingest_quotas.surql"),
    },
    Migration {
        version: 23,
        name: "recording_lifecycle",
        filename: "0023_recording_lifecycle.surql",
        sql: include_str!("../migrations/0023_recording_lifecycle.surql"),
    },
    Migration {
        version: 24,
        name: "work_context_governance",
        filename: "0024_work_context_governance.surql",
        sql: include_str!("../migrations/0024_work_context_governance.surql"),
    },
    Migration {
        version: 25,
        name: "map_authoring",
        filename: "0025_map_authoring.surql",
        sql: include_str!("../migrations/0025_map_authoring.surql"),
    },
    Migration {
        version: 26,
        name: "map_authoring_products",
        filename: "0026_map_authoring_products.surql",
        sql: include_str!("../migrations/0026_map_authoring_products.surql"),
    },
];

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
}

impl SchemaStatus {
    pub fn is_current(&self) -> bool {
        self.pending_versions.is_empty() && self.current_version == Some(self.latest_version)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationReport {
    pub applied_versions: Vec<u32>,
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
    validate_catalog(&MIGRATIONS)?;
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
        let Some(expected) = MIGRATIONS.get(version as usize) else {
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
    let pending_versions = MIGRATIONS
        .iter()
        .filter(|migration| !by_version.contains_key(&migration.version))
        .map(|migration| migration.version)
        .collect();
    Ok(SchemaStatus {
        current_version,
        latest_version: MIGRATIONS.last().expect("catalog is non-empty").version,
        pending_versions,
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
        Ok(validate_history(&applied)?)
    }

    pub async fn migrate(&self) -> Result<MigrationReport, StoreError> {
        self.require_root("schema migration")?;
        validate_catalog(&MIGRATIONS)?;

        self.db.query(MIGRATIONS[0].sql).await?.check()?;
        let mut history = self.migration_history().await?;
        let status = validate_history(&history)?;
        let mut applied_versions = Vec::new();

        for version in status.pending_versions.clone() {
            let migration = &MIGRATIONS[version as usize];
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
                .and_then(|response| response.check());

            if let Err(error) = result {
                // A second replica may have committed the same migration first.
                // Only accept that race when the durable checksum matches exactly.
                history = self.migration_history().await?;
                match validate_history(&history) {
                    Ok(current) if !current.pending_versions.contains(&version) => {
                        continue;
                    }
                    _ => return Err(StoreError::Database(error)),
                }
            }
            applied_versions.push(version);
        }

        history = self.migration_history().await?;
        let status = validate_history(&history)?;
        Ok(MigrationReport {
            applied_versions,
            status,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::PlatformTable;

    const RELATIONS: [&str; 9] = [
        "membership",
        "artifact_grant",
        "profile_server",
        "task_produced_artifact",
        "task_used_artifact",
        "artifact_derived_from",
        "task_used_frame",
        "recording_segment",
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
