//! Pure history checks and bounded, pinned Docker qualification of the actual executor.
use super::*;
use crate::runner::prepare;
use crate::*;
use std::time::Duration;
use surrealdb::{
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};

#[path = "../../../../testing/fixtures/store/container.rs"]
mod container;

const BASE: &str = "DEFINE TABLE fixture_marker SCHEMAFULL PERMISSIONS NONE; DEFINE FIELD value ON fixture_marker TYPE string; CREATE fixture_marker:base SET value = 'base';";
const ADVANCE: &str = "CREATE fixture_marker:advanced SET value = 'advanced';";
const FEATURE: &str = "DEFINE TABLE fixture_optional SCHEMAFULL PERMISSIONS NONE; DEFINE FIELD value ON fixture_optional TYPE string; CREATE fixture_optional:first SET value = 'first';";

fn name(value: &str) -> ModuleName {
    ModuleName::new(value).unwrap()
}
fn migration(
    version: u32,
    label: &str,
    sql: &'static str,
    requirements: Vec<LaneRequirement>,
) -> Migration {
    Migration::new(
        MigrationVersion::new(version),
        MigrationName::new(label).unwrap(),
        sql,
    )
    .unwrap()
    .with_requirements(requirements)
    .unwrap()
}
fn module(
    label: &str,
    table: &str,
    layer: ModuleLayer,
    migrations: Vec<Migration>,
    requires: Vec<LaneRequirement>,
) -> ModuleSetup {
    ModuleSetup::builder(name(label), layer)
        .ownership(vec![OwnershipClaim::Table(TableName::new(table).unwrap())])
        .lane(MigrationLane::new(migrations).unwrap())
        .execution(
            LaneExecution::new(
                ExecutionImage::new("native-fixture").unwrap(),
                ExecutionCommand::new(vec!["native-fixture".into()]).unwrap(),
            )
            .unwrap(),
        )
        .requires(requires)
        .build()
        .unwrap()
}
fn registry(base: Vec<Migration>, optional: Vec<Migration>) -> ModuleRegistry {
    ModuleRegistry::new(vec![
        module("store", "fixture_marker", ModuleLayer::Kernel, base, vec![]),
        module(
            "feature",
            "fixture_optional",
            ModuleLayer::Optional,
            optional,
            vec![LaneRequirement::Satisfied(name("store"))],
        ),
    ])
    .unwrap()
}
fn row(module: &ModuleName, migration: &Migration, requirements: Vec<Receipt>) -> Applied {
    Applied {
        id: migration_id(module, migration.version()),
        module: module.as_str().into(),
        version: i64::from(migration.version().get()),
        name: migration.name().as_str().into(),
        filename: migration.filename().into(),
        checksum: digest(migration.sql().as_bytes()),
        requires_checksum: requires_checksum(migration),
        requirements,
    }
}
fn header(module: &str) -> Header {
    Header {
        id: header_id(&name(module)),
        module: module.into(),
        initialized: true,
    }
}

#[test]
fn empty_header_is_distinct_from_absent_and_disabled_history_is_preserved() {
    let catalog = registry(vec![], vec![]);
    let prepared = prepare(catalog.select(vec![]).unwrap()).unwrap();
    let absent = prepared.validate_history(&[], &[]).unwrap();
    assert!(!absent.is_current());
    assert_eq!(absent.lane(&name("store")).unwrap().current, None);
    let initialized = prepared
        .validate_history(&[header("store"), header("feature")], &[])
        .unwrap();
    assert!(initialized.is_current());
    assert!(initialized.lane(&name("feature")).unwrap().initialized);
    assert!(!initialized.lane(&name("feature")).unwrap().selected);
}
#[test]
fn unknown_duplicate_gap_ahead_and_drift_histories_fail() {
    let first = migration(0, "first", BASE, vec![]);
    let second = migration(1, "second", ADVANCE, vec![]);
    let catalog = registry(vec![first.clone(), second.clone()], vec![]);
    let prepared = prepare(catalog.select(vec![]).unwrap()).unwrap();
    assert!(
        prepared
            .validate_history(&[header("unknown")], &[])
            .is_err()
    );
    assert!(
        prepared
            .validate_history(&[header("store"), header("store")], &[])
            .is_err()
    );
    assert!(
        prepared
            .validate_history(&[], &[row(&name("store"), &first, vec![])])
            .is_err()
    );
    assert!(
        prepared
            .validate_history(&[header("store")], &[row(&name("store"), &second, vec![])])
            .is_err()
    );
    let future = migration(2, "future", ADVANCE, vec![]);
    assert!(
        prepared
            .validate_history(&[header("store")], &[row(&name("store"), &future, vec![])])
            .is_err()
    );
    for change in [0, 1, 2, 3, 4] {
        let mut changed = row(&name("store"), &first, vec![]);
        match change {
            0 => changed.name.push('x'),
            1 => changed.filename.push('x'),
            2 => changed.checksum.push('x'),
            3 => changed.requires_checksum.push('x'),
            _ => changed.id = RecordId::new(MIGRATION_TABLE, "wrong"),
        }
        assert!(
            prepared
                .validate_history(&[header("store")], &[changed])
                .is_err()
        );
    }
}
#[test]
fn old_satisfied_receipt_survives_appended_dependency_and_cannot_claim_unapplied_version() {
    let first = migration(0, "first", BASE, vec![]);
    let dependent = migration(
        0,
        "dependent",
        FEATURE,
        vec![LaneRequirement::Satisfied(name("store"))],
    );
    let catalog = registry(
        vec![first.clone(), migration(1, "advance", ADVANCE, vec![])],
        vec![dependent.clone()],
    );
    let prepared = prepare(catalog.select(vec![name("feature")]).unwrap()).unwrap();
    let headers = [header("store"), header("feature")];
    let mut rows = vec![
        row(&name("store"), &first, vec![]),
        row(
            &name("feature"),
            &dependent,
            vec![Receipt {
                module: "store".into(),
                version: Some(0),
            }],
        ),
    ];
    assert!(prepared.validate_history(&headers, &rows).is_ok());
    rows[1].requirements[0].version = Some(1);
    assert!(prepared.validate_history(&headers, &rows).is_err());
}

async fn fixture() -> (container::Container, Surreal<Client>, Surreal<Client>) {
    let password = uuid::Uuid::now_v7().to_string();
    let (container, endpoint) =
        container::Container::start(container::Docker::default(), "memory", &password)
            .await
            .unwrap();
    let endpoint = endpoint
        .strip_prefix("ws://")
        .expect("fixture emits a WebSocket endpoint");
    let connect = || async {
        let mut last_error = "connection not attempted".to_owned();
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                match Surreal::new::<Ws>(endpoint).await {
                    Ok(db) => match db
                        .signin(Root {
                            username: "fixture_admin".into(),
                            password: password.clone(),
                        })
                        .await
                    {
                        Ok(_) => {
                            db.query("DEFINE NAMESPACE IF NOT EXISTS module_runner;")
                                .await
                                .expect("fixture namespace definition")
                                .check()
                                .expect("fixture namespace admission");
                            db.use_ns("module_runner")
                                .await
                                .expect("fixture namespace selection");
                            db.query("DEFINE DATABASE IF NOT EXISTS fixture;")
                                .await
                                .expect("fixture database definition")
                                .check()
                                .expect("fixture database admission");
                            db.use_db("fixture")
                                .await
                                .expect("fixture database selection");
                            break db;
                        }
                        Err(error) => {
                            last_error = format!(
                                "authentication: {}",
                                error.to_string().replace(&password, "[REDACTED]")
                            )
                        }
                    },
                    Err(error) => {
                        last_error = format!(
                            "connection: {}",
                            error.to_string().replace(&password, "[REDACTED]")
                        )
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;
        result.unwrap_or_else(|_| {
            panic!("isolated module fixture readiness exceeded 30 seconds: {last_error}")
        })
    };
    let a = connect().await;
    let b = connect().await;
    (container, a, b)
}
async fn markers(db: &Surreal<Client>, table: &'static str) -> Vec<RecordId> {
    let mut response = db
        .query("SELECT VALUE id FROM type::table($table) ORDER BY id;")
        .bind(("table", table))
        .await
        .unwrap()
        .check()
        .unwrap();
    response.take(0).unwrap()
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_concurrent_jobs_disabled_preservation_later_enable_and_upgrade() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, a, b) = fixture().await;
        let base = migration(0, "base", BASE, vec![]);
        let feature = migration(
            0,
            "feature",
            FEATURE,
            vec![LaneRequirement::Satisfied(name("store"))],
        );
        let catalog = registry(vec![base.clone()], vec![feature.clone()]);
        let prepared = prepare(catalog.select(vec![]).unwrap()).unwrap();
        let (left, right) = tokio::join!(prepared.apply(&a), prepared.apply(&b));
        assert!(left.unwrap().is_current());
        assert!(right.unwrap().is_current());
        assert_eq!(markers(&a, "fixture_marker").await.len(), 1);
        assert!(
            !prepared
                .status(&a)
                .await
                .unwrap()
                .lane(&name("feature"))
                .unwrap()
                .initialized
        );
        let enabled = prepare(catalog.select(vec![name("feature")]).unwrap()).unwrap();
        assert!(enabled.apply(&a).await.unwrap().is_current());
        assert_eq!(markers(&a, "fixture_optional").await.len(), 1);
        let upgraded = registry(
            vec![base, migration(1, "advance", ADVANCE, vec![])],
            vec![feature],
        );
        let disabled = prepare(upgraded.select(vec![]).unwrap()).unwrap();
        assert!(disabled.apply(&a).await.unwrap().is_current());
        let status = disabled.status(&a).await.unwrap();
        assert!(status.lane(&name("feature")).unwrap().initialized);
        assert!(!status.lane(&name("feature")).unwrap().selected);
        assert_eq!(markers(&a, "fixture_optional").await.len(), 1);
        assert_eq!(markers(&a, "fixture_marker").await.len(), 2);
        assert!(
            prepare(upgraded.select(vec![name("feature")]).unwrap())
                .unwrap()
                .apply(&a)
                .await
                .unwrap()
                .is_current()
        );
    })
    .await
    .expect("native lane installation exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_failed_body_rolls_back_and_unknown_history_blocks_pending_effects() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, a, _b) = fixture().await;
        let first = migration(0, "base", BASE, vec![]);
        let baseline = registry(vec![first.clone()], vec![]);
        prepare(baseline.select(vec![]).unwrap()).unwrap().apply(&a).await.unwrap();
        let failure = registry(vec![first.clone(), migration(1, "fail", "CREATE fixture_marker:rolled_back SET value = 'private-token'; THROW 'private-token';", vec![])], vec![]);
        let prepared = prepare(failure.select(vec![]).unwrap()).unwrap();
        let error = prepared.apply(&a).await.unwrap_err();
        assert!(!format!("{error:?} {error}").contains("private-token"));
        assert_eq!(markers(&a, "fixture_marker").await.len(), 1);
        assert_eq!(prepared.status(&a).await.unwrap().lane(&name("store")).unwrap().current, Some(MigrationVersion::new(0)));
        a.query("CREATE platform_module_lane:unknown SET module = 'unknown', initialized = true;").await.unwrap().check().unwrap();
        let pending = registry(vec![first, migration(1, "advance", ADVANCE, vec![])], vec![]);
        let prepared = prepare(pending.select(vec![]).unwrap()).unwrap();
        assert!(prepared.apply(&a).await.is_err());
        assert_eq!(markers(&a, "fixture_marker").await.len(), 1);
    }).await.expect("native rollback qualification exceeded 120 seconds");
}

#[test]
fn explicit_prerequisite_identity_does_not_reinterpret_old_history() {
    let first = migration(0, "base", BASE, vec![]);
    let dependent = migration(
        0,
        "feature",
        FEATURE,
        vec![LaneRequirement::AtLeast {
            module: name("store"),
            version: MigrationVersion::new(0),
        }],
    );
    let changed = migration(
        0,
        "feature",
        FEATURE,
        vec![LaneRequirement::AtLeast {
            module: name("store"),
            version: MigrationVersion::new(1),
        }],
    );
    let catalog = registry(
        vec![first.clone(), migration(1, "advance", ADVANCE, vec![])],
        vec![changed],
    );
    let prepared = prepare(catalog.select(vec![name("feature")]).unwrap()).unwrap();
    assert!(
        prepared
            .validate_history(
                &[header("store"), header("feature")],
                &[
                    row(&name("store"), &first, vec![]),
                    row(
                        &name("feature"),
                        &dependent,
                        vec![Receipt {
                            module: "store".into(),
                            version: Some(0)
                        }]
                    ),
                ]
            )
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_incompatible_or_uncommitted_infrastructure_is_not_a_winner() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, a, _b) = fixture().await;
        let catalog = registry(vec![migration(0, "base", BASE, vec![])], vec![]);
        let prepared = prepare(catalog.select(vec![]).unwrap()).unwrap();
        let tx = a.clone().begin().await.unwrap();
        tx.query(INFRASTRUCTURE).await.unwrap().check().unwrap();
        tx.cancel().await.unwrap();
        assert!(!infrastructure_state(&a, ExecutionLimits::default()).await.unwrap());
        a.query("DEFINE TABLE platform_module_lane SCHEMALESS PERMISSIONS FULL; DEFINE TABLE platform_module_migration SCHEMALESS PERMISSIONS FULL;").await.unwrap().check().unwrap();
        assert!(prepared.apply(&a).await.is_err());
        let mut response = a.query("INFO FOR DB;").await.unwrap();
        let info = response.take::<Option<DatabaseInfo>>(0).unwrap().expect("database metadata");
        assert!(!info.tables.contains_key("fixture_marker"));
    }).await.expect("native incompatible infrastructure qualification exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_transaction_timeout_and_dropped_awaiter_cancel_before_commit() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, a, _b) = fixture().await;
        let first = migration(0, "base", BASE, vec![]);
        let catalog = registry(vec![first], vec![]);
        prepare(catalog.select(vec![]).unwrap())
            .unwrap()
            .apply(&a)
            .await
            .unwrap();
        // SLEEP is intentionally a private cleanup fixture, not admitted migration SQL.
        // The native helper is the same one used after production preflight.
        let delayed = migration(
            1,
            "delayed",
            "CREATE fixture_marker:cancelled SET value = 'cancelled'; SLEEP 2s;",
            vec![],
        );
        let result = transaction::execute(
            &a,
            transaction::Operation::Migration {
                sql: delayed.sql(),
                content: row(&name("store"), &delayed, vec![]),
            },
            ExecutionLimits {
                operation_timeout: Duration::from_millis(200),
                cancel_timeout: Duration::from_secs(10),
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(markers(&a, "fixture_marker").await.len(), 1);
        let connection = a.clone();
        let content = row(&name("store"), &delayed, vec![]);
        let caller = tokio::spawn(async move {
            transaction::execute(
                &connection,
                transaction::Operation::Migration {
                    sql: delayed.sql(),
                    content,
                },
                ExecutionLimits::default(),
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        // The cleanup task owns its native handle after the awaiter is dropped.
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(markers(&a, "fixture_marker").await.len(), 1);
        assert!(
            prepare(catalog.select(vec![]).unwrap())
                .unwrap()
                .status(&a)
                .await
                .unwrap()
                .is_current()
        );
    })
    .await
    .expect("native transaction cancellation qualification exceeded 120 seconds");
}
