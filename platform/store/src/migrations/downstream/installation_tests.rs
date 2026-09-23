//! Requires Docker and the locally available, digest-pinned SurrealDB 3.2.4 image.
//! Owns its loopback-only container and never reads installation credentials.
use std::{process::Command, time::Duration};

use super::*;
use crate::{StoreConfig, StoreCredentials};

const IMAGE: &str =
    "surrealdb/surrealdb@sha256:51baed8709f57f67dcf04b30e3177db846803fa9342dae2be58c6fa5f8d59843";
struct Fixture(String);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new("timeout")
            .args(["20", "docker", "rm", "--force", &self.0])
            .output();
    }
}
impl Fixture {
    async fn start() -> (Self, PlatformStore) {
        let fixture = Self(format!(
            "veoveo-fork-migrations-{}",
            uuid::Uuid::now_v7().simple()
        ));
        let password = uuid::Uuid::now_v7().to_string();
        let output = Command::new("timeout")
            .args([
                "30",
                "docker",
                "run",
                "--detach",
                "--rm",
                "--pull",
                "never",
                "--name",
                &fixture.0,
                "--runtime",
                "runc",
                "--memory",
                "2g",
                "--cpus",
                "2",
                "--pids-limit",
                "256",
                "--publish",
                "127.0.0.1::8000",
                "--env",
                "SURREAL_USER",
                "--env",
                "SURREAL_PASS",
                IMAGE,
                "start",
                "--log",
                "error",
                "memory",
            ])
            .env("SURREAL_USER", "fixture_admin")
            .env("SURREAL_PASS", &password)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "pinned disposable SurrealDB could not start"
        );
        let port = Command::new("timeout")
            .args(["10", "docker", "port", &fixture.0, "8000/tcp"])
            .output()
            .unwrap();
        assert!(port.status.success());
        let port = String::from_utf8(port.stdout).unwrap();
        let endpoint = format!("ws://{}", port.trim());
        assert!(endpoint.starts_with("ws://127.0.0.1:"));
        let config = StoreConfig::builder(
            endpoint,
            "fork_migration_tests",
            "fixture",
            StoreCredentials::root("fixture_admin", password),
        )
        .build()
        .unwrap();
        let store = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let Ok(store) = PlatformStore::connect(config.clone()).await {
                    break store;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("disposable database readiness exceeded 30 seconds");
        (fixture, store)
    }
}

const UPSTREAM: [Migration; 3] = [
    Migration {
        version: 0,
        name: "history",
        filename: "0000_history.surql",
        sql: include_str!("../../../migrations/0000_schema_migrations.surql"),
    },
    Migration {
        version: 1,
        name: "fork_history",
        filename: "0001_fork_history.surql",
        sql: include_str!("../../../migrations/0092_downstream_migrations.surql"),
    },
    Migration {
        version: 2,
        name: "upstream_advance",
        filename: "0002_upstream_advance.surql",
        sql: "CREATE fixture_marker:upstream;",
    },
];
const FORK: [DownstreamMigration; 2] = [
    DownstreamMigration {
        migration: Migration {
            version: 0,
            name: "fork_first",
            filename: "0000_fork_first.surql",
            sql: "CREATE fixture_marker:fork_first;",
        },
        requires_upstream: 1,
    },
    DownstreamMigration {
        migration: Migration {
            version: 1,
            name: "fork_second",
            filename: "0001_fork_second.surql",
            sql: "CREATE fixture_marker:fork_second;",
        },
        requires_upstream: 2,
    },
];

async fn markers(store: &PlatformStore) -> Vec<RecordId> {
    store
        .db
        .query("SELECT VALUE id FROM fixture_marker ORDER BY id;")
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}

#[tokio::test]
#[ignore = "requires Docker and the pinned SurrealDB 3.2.4 image"]
async fn fork_upgrade_rollback_and_drift_use_the_production_executor() {
    let (_fixture, store) = Fixture::start().await;
    tokio::time::timeout(Duration::from_secs(90), async {
        // A missing dependency must fail even before bootstrapping history.
        assert!(matches!(store.migrate_catalogs(&UPSTREAM[..2], &FORK).await,
            Err(StoreError::DownstreamMigration(DownstreamMigrationError::MissingUpstream { .. }))));
        assert!(store.migration_history().await.is_err());
        let first = store.migrate_catalogs(&UPSTREAM[..2], &FORK[..1]).await.unwrap();
        assert_eq!(first.applied_versions, [0, 1]);
        assert_eq!(first.downstream_applied_versions, [0]);
        let history = store.migration_history().await.unwrap();
        let fork_history = store.downstream_history(false).await.unwrap();
        assert_eq!(markers(&store).await.len(), 1);

        // Catalog removal and every persisted field mismatch reject the pending
        // upstream side effect, rather than discovering drift after upgrading.
        assert!(store.migrate_catalogs(&UPSTREAM, &[]).await.is_err());
        for field in 0..4 {
            let mut edited = FORK[0];
            match field {
                0 => edited.migration.sql = "CREATE fixture_marker:edited;",
                1 => edited.migration.name = "renamed",
                2 => edited.migration.filename = "0000_renamed.surql",
                _ => edited.requires_upstream = 0,
            }
            assert!(matches!(store.migrate_catalogs(&UPSTREAM, &[edited]).await,
                Err(StoreError::DownstreamMigration(DownstreamMigrationError::Drift { version: 0 }))));
            assert_eq!(store.migration_history().await.unwrap(), history);
            assert_eq!(markers(&store).await.len(), 1);
        }
        let failed = DownstreamMigration { migration: Migration {
            sql: "CREATE fixture_marker:rollback; THROW 'intentional migration fixture failure';",
            ..FORK[1].migration
        }, ..FORK[1] };
        assert!(matches!(store.migrate_catalogs(&UPSTREAM, &[FORK[0], failed]).await,
            Err(StoreError::DownstreamMigrationExecution { version: 1, .. })));
        assert_eq!(store.downstream_history(false).await.unwrap().len(), 1);
        assert_eq!(markers(&store).await.len(), 2); // only the upstream advance committed

        // Two replicas running the same append-only catalogs may accept a
        // matching committed winner; both must preserve the first fork step.
        let (a, b) = tokio::join!(store.migrate_catalogs(&UPSTREAM, &FORK), store.migrate_catalogs(&UPSTREAM, &FORK));
        assert!(a.unwrap().status.is_current());
        assert!(b.unwrap().status.is_current());
        assert_eq!(markers(&store).await.len(), 3);
        let after = store.downstream_history(false).await.unwrap();
        assert_eq!(after[0].checksum, fork_history[0].checksum);
        assert_eq!(after[0].applied_at, fork_history[0].applied_at);
        let repeated = store.migrate_catalogs(&UPSTREAM, &FORK).await.unwrap();
        assert!(repeated.applied_versions.is_empty());
        assert!(repeated.downstream_applied_versions.is_empty());
        // Both histories contain numeric version 1 without collisions.
        assert_eq!(after[1].version, history[1].version);
    }).await.expect("fork migration scenarios exceeded 90 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and the pinned SurrealDB 3.2.4 image"]
async fn production_upgrade_preserves_existing_history_and_runtime_authority() {
    let (_fixture, store) = Fixture::start().await;
    tokio::time::timeout(Duration::from_secs(120), async {
        let upstream = &super::super::MIGRATIONS;
        store.migrate_catalogs(&upstream[..92], &[]).await.unwrap();
        let before = store.migration_history().await.unwrap();
        let report = store.migrate().await.unwrap();
        assert_eq!(report.applied_versions, [92]);
        assert!(report.downstream_applied_versions.is_empty());
        assert!(report.status.is_current());
        assert_eq!(
            &store.migration_history().await.unwrap()[..92],
            before.as_slice()
        );
        assert!(store.migrate().await.unwrap().applied_versions.is_empty());
        assert!(store.schema_status().await.unwrap().is_current());
        store
            .db
            .query("REMOVE TABLE platform_downstream_migration;")
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(store.schema_status().await.is_err());
        assert!(store.migrate().await.is_err());
        store
            .replace_database_editor("runtime", &"fixture_runtime_password".into())
            .await
            .unwrap();
        let config = StoreConfig::builder(
            store.config().endpoint().as_str(),
            "fork_migration_tests",
            "fixture",
            StoreCredentials::database("runtime", "fixture_runtime_password"),
        )
        .build()
        .unwrap();
        let runtime = PlatformStore::connect(config).await.unwrap();
        assert!(matches!(
            runtime.migrate().await,
            Err(StoreError::RootCredentialsRequired { .. })
        ));
    })
    .await
    .expect("production migration upgrade exceeded 120 seconds");
}
