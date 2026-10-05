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
                preparation: None,
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
                    preparation: None,
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

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_preparation_generation_fences_delayed_rotation_and_conflicting_identity() {
    use crate::runner::DatabaseEditorCredentials;
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, a, b) = fixture().await;
        let registry = registry(vec![], vec![]);
        let prepared = prepare(registry.select(vec![]).unwrap()).unwrap();
        let old =
            PreparationKey::new(InstallationGeneration::new(1).unwrap(), "a".repeat(64)).unwrap();
        let current =
            PreparationKey::new(InstallationGeneration::new(2).unwrap(), "b".repeat(64)).unwrap();
        let conflict = PreparationKey::new(current.generation(), "c".repeat(64)).unwrap();
        prepared.claim_preparation(&a, &old).await.unwrap();
        assert!(prepared.require_preparation(&a, &old).await.is_err());
        // A second preparer advances while the first still owns its schema work.
        prepared.claim_preparation(&b, &current).await.unwrap();
        assert!(
            prepared
                .complete_preparation(
                    &a,
                    &old,
                    DatabaseEditorCredentials::new("runtime", "stale-password").unwrap()
                )
                .await
                .is_err()
        );
        assert!(prepared.claim_preparation(&a, &conflict).await.is_err());
        prepared
            .complete_preparation(
                &b,
                &current,
                DatabaseEditorCredentials::new("runtime", "current-password").unwrap(),
            )
            .await
            .unwrap();
        prepared.require_preparation(&a, &current).await.unwrap();
        let stale_header = transaction::execute(
            &a,
            transaction::Operation::Header {
                content: header("store"),
                preparation: Some(old.clone()),
            },
            Default::default(),
        )
        .await;
        assert!(stale_header.is_err());
        assert!(
            markers(&a, LANE_TABLE).await.is_empty(),
            "stale installation transaction must not initialize a lane"
        );

        assert!(prepared.require_preparation(&a, &old).await.is_err());
        assert!(prepared.claim_preparation(&a, &old).await.is_err());
        // Repeating completion does not overwrite credentials for an already completed key.
        prepared
            .complete_preparation(
                &a,
                &current,
                DatabaseEditorCredentials::new("runtime", "different-password").unwrap(),
            )
            .await
            .unwrap();
        b.signin(surrealdb::opt::auth::Database {
            namespace: "module_runner".into(),
            database: "fixture".into(),
            username: "runtime".into(),
            password: "current-password".into(),
        })
        .await
        .unwrap();
        b.query("RETURN true;").await.unwrap().check().unwrap();
    })
    .await
    .expect("preparation generation fixture exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_reciprocal_kernel_field_types_allow_forward_definition_and_check_values() {
    tokio::time::timeout(Duration::from_secs(120), async {
        for reverse in [false, true] {
            let (_fixture, db, _other) = fixture().await;
            let left = module(
                "left",
                "fixture_left",
                ModuleLayer::Kernel,
                vec![migration(0, "initial", "DEFINE TABLE fixture_left SCHEMAFULL; DEFINE FIELD link ON fixture_left TYPE record<fixture_right>;", vec![])],
                vec![],
            );
            let right = module(
                "right",
                "fixture_right",
                ModuleLayer::Kernel,
                vec![migration(0, "initial", "DEFINE TABLE fixture_right SCHEMAFULL; DEFINE FIELD link ON fixture_right TYPE record<fixture_left>;", vec![])],
                vec![],
            );
            let registry = ModuleRegistry::new(if reverse { vec![right, left] } else { vec![left, right] }).unwrap();
            let selection = registry.select(vec![]).unwrap();
            assert_eq!(selection.ordered()[0].name().as_str(), if reverse { "right" } else { "left" });
            prepare(selection).unwrap().apply(&db).await.unwrap();
            db.query("CREATE fixture_left:one SET link = fixture_right:one; CREATE fixture_right:one SET link = fixture_left:one;")
                .await.unwrap().check().unwrap();
            assert_eq!(markers(&db, "fixture_left").await.len(), 1);
            assert_eq!(markers(&db, "fixture_right").await.len(), 1);
            assert!(db.query("CREATE fixture_left:wrong SET link = fixture_left:one;").await.unwrap().check().is_err());
            assert!(db.query("CREATE fixture_right:wrong SET link = fixture_right:one;").await.unwrap().check().is_err());
            assert_eq!(markers(&db, "fixture_left").await.len(), 1);
            assert_eq!(markers(&db, "fixture_right").await.len(), 1);
        }
    }).await.expect("reciprocal field schema qualification exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_invalid_field_link_selection_has_no_bookkeeping_or_owner_effects() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, db, _other) = fixture().await;
        let mut before = db.query("INFO FOR DB;").await.unwrap().check().unwrap();
        let before: surrealdb::types::Value = before.take(0).unwrap();
        let registry = ModuleRegistry::new(vec![
            module("a_valid", "fixture_left", ModuleLayer::Kernel, vec![migration(0, "initial", "DEFINE TABLE fixture_left; CREATE fixture_left:one;", vec![])], vec![]),
            module("z_invalid", "fixture_right", ModuleLayer::Kernel, vec![migration(0, "initial", "DEFINE TABLE fixture_right; DEFINE FIELD link ON fixture_right TYPE array<record<fixture_left> | record<unclaimed>>;", vec![])], vec![]),
        ]).unwrap();
        assert!(prepare(registry.select(vec![]).unwrap()).is_err());
        let mut after = db.query("INFO FOR DB;").await.unwrap().check().unwrap();
        let after: surrealdb::types::Value = after.take(0).unwrap();
        assert_eq!(before, after);
    }).await.expect("invalid field selection preflight exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_owned_update_api_and_caller_settle_or_roll_back_in_one_transaction() {
    const API: &str = "DEFINE FUNCTION fn::kernel::store::update_v1($id: record<fixture_marker>, $value: string) -> bool { UPDATE ONLY $id SET value = $value RETURN NONE; RETURN true; } PERMISSIONS FULL;";
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, db, _other) = fixture().await;
        let sql = Box::leak(format!("{BASE} {API}").into_boxed_str());
        let owner = module("store", "fixture_marker", ModuleLayer::Kernel, vec![migration(0, "initial", sql, vec![])], vec![]);
        let api = KernelSqlApi::new(
            FunctionName::new("fn::kernel::store::update_v1").unwrap(), MigrationVersion::new(0),
            SqlSignature::new(vec![SqlParameter::new("id", SqlType::Record(TableName::new("fixture_marker").unwrap())).unwrap(), SqlParameter::new("value", SqlType::String).unwrap()], SqlType::Bool).unwrap(),
            SqlReadProfile::new(vec![TableName::new("fixture_marker").unwrap()]).unwrap(), API,
        ).unwrap().with_effects(SqlEffectProfile::OwnedUpdate(SqlUpdateProfile::new(TableName::new("fixture_marker").unwrap(), vec![SqlFieldName::new("value").unwrap()]).unwrap()));
        let owner = ModuleSetup::builder(owner.name().clone(), owner.layer())
            .ownership(vec![OwnershipClaim::Table(TableName::new("fixture_marker").unwrap()), OwnershipClaim::Function(api.name().clone())])
            .execution(owner.execution().clone()).lane(owner.lane().clone()).sql_apis(vec![api]).build().unwrap();
        let prerequisite = LaneRequirement::AtLeast { module: name("store"), version: MigrationVersion::new(0) };
        let failed = module("consumer", "fixture_optional", ModuleLayer::Optional,
            vec![migration(0, "settle", "fn::kernel::store::update_v1(fixture_marker:base,'released'); DEFINE TABLE fixture_optional; CREATE fixture_optional:one SET value='settled'; THROW 'controlled failure';", vec![prerequisite.clone()])], vec![prerequisite.clone()]);
        let registry = ModuleRegistry::new(vec![owner.clone(), failed]).unwrap();
        let prepared = prepare(registry.select(vec![name("consumer")]).unwrap()).unwrap();
        assert!(prepared.apply(&db).await.is_err());
        let mut values = db.query("SELECT VALUE value FROM fixture_marker;").await.unwrap().check().unwrap();
        let values: Vec<String> = values.take(0).unwrap();
        assert_eq!(values, vec!["base"]);
        let absent = db.query("SELECT * FROM fixture_optional;").await.unwrap().check().unwrap_err();
        assert!(absent.to_string().contains("does not exist"));
        assert_eq!(prepared.status(&db).await.unwrap().lane(&name("consumer")).unwrap().current, None);
        let successful = module("consumer", "fixture_optional", ModuleLayer::Optional,
            vec![migration(0, "settle", "fn::kernel::store::update_v1(fixture_marker:base,'released'); DEFINE TABLE fixture_optional; CREATE fixture_optional:one SET value='settled';", vec![prerequisite.clone()])], vec![prerequisite]);
        let registry = ModuleRegistry::new(vec![owner, successful]).unwrap();
        let prepared = prepare(registry.select(vec![name("consumer")]).unwrap()).unwrap();
        prepared.apply(&db).await.unwrap();
        let mut values = db.query("SELECT VALUE value FROM fixture_marker;").await.unwrap().check().unwrap();
        let values: Vec<String> = values.take(0).unwrap();
        assert_eq!(values, vec!["released"]);
        assert_eq!(markers(&db, "fixture_optional").await.len(), 1);
        assert_eq!(prepared.status(&db).await.unwrap().lane(&name("consumer")).unwrap().current, Some(MigrationVersion::new(0)));
    }).await.expect("owned update API transaction qualification exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_current_schema_values_views_and_guarded_events() {
    const SQL: &str = "
DEFINE FUNCTION fn::store::first($value: string) -> string { RETURN fn::store::second($value); };
DEFINE FUNCTION fn::store::second($value: string) -> string { RETURN string::lowercase($value); };
DEFINE FUNCTION fn::store::exists($id: record<fixture_marker>) -> bool { RETURN record::exists($id); };
DEFINE TABLE fixture_marker SCHEMAFULL;
DEFINE FIELD state ON fixture_marker TYPE 'queued' | 'running';
DEFINE FIELD tags ON fixture_marker TYPE array<string> VALUE $value.distinct() ASSERT $value.all(|$item: any| string::len($item)>0);
DEFINE FIELD digest ON fixture_marker TYPE string ASSERT $value.len()=64;
DEFINE FIELD sealed ON fixture_marker TYPE {nonce:string, version:int};
DEFINE FIELD target ON fixture_marker TYPE option<record> ASSERT $value=NONE OR record::tb($value)!='';
DEFINE FIELD nonce ON fixture_marker TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD replacement ON fixture_marker TYPE option<uuid> ASSERT $value=NONE OR $value!=$this.nonce;
DEFINE FIELD sequence ON fixture_marker TYPE int DEFAULT 0;
DEFINE TABLE fixture_uuid SCHEMAFULL;
DEFINE FIELD id ON fixture_uuid TYPE uuid DEFAULT rand::uuid();
DEFINE TABLE fixture_relation TYPE RELATION IN fixture_marker OUT fixture_marker ENFORCED;
DEFINE TABLE fixture_daily TYPE ANY SCHEMALESS AS SELECT state, count() AS count FROM fixture_marker GROUP BY state;
DEFINE TABLE fixture_head SCHEMAFULL;
DEFINE FIELD last_sequence ON fixture_head TYPE int;
CREATE fixture_head:current SET last_sequence=0;
DEFINE EVENT advance ON fixture_marker WHEN $after.sequence>0 AND $before.sequence!=$after.sequence THEN {
    LET $head=SELECT * FROM ONLY fixture_head:current;
    IF type::is_object($head) THEN {
        IF $after.sequence<=$head.last_sequence { THROW 'sequence conflict'; };
        UPDATE ONLY fixture_head:current SET last_sequence=$after.sequence;
    } ELSE { THROW 'missing head'; } END;
};
CREATE fixture_marker:one SET state=fn::store::first('QUEUED'), tags=['pin','pin'], digest=crypto::sha256('input'), sealed={nonce:'opaque',version:1}, target=fixture_other:one, sequence=1;
CREATE fixture_uuid;
";
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, db, _other) = fixture().await;
        let catalog = ModuleRegistry::new(vec![
            ModuleSetup::builder(name("store"), ModuleLayer::Kernel)
                .ownership(vec![
                    OwnershipClaim::TablePrefix(TablePrefix::new("fixture_").unwrap()),
                    OwnershipClaim::FunctionPrefix(FunctionPrefix::new("fn::store::").unwrap()),
                ])
                .execution(
                    LaneExecution::new(
                        ExecutionImage::new("native-fixture").unwrap(),
                        ExecutionCommand::new(vec!["native-fixture".into()]).unwrap(),
                    )
                    .unwrap(),
                )
                .lane(MigrationLane::new(vec![migration(0, "profile", SQL, vec![])]).unwrap())
                .build()
                .unwrap(),
        ])
        .unwrap();
        prepare(catalog.select(vec![]).unwrap())
            .unwrap()
            .apply(&db)
            .await
            .unwrap();
        let mut tags = db
            .query("SELECT VALUE tags FROM fixture_marker;")
            .await
            .unwrap()
            .check()
            .unwrap();
        let tags: Vec<Vec<String>> = tags.take(0).unwrap();
        assert_eq!(tags, vec![vec!["pin"]]);
        let mut head = db
            .query("SELECT VALUE last_sequence FROM fixture_head;")
            .await
            .unwrap()
            .check()
            .unwrap();
        let head: Vec<i64> = head.take(0).unwrap();
        assert_eq!(head, vec![1]);
        let mut counts = db
            .query("SELECT VALUE count FROM fixture_daily;")
            .await
            .unwrap()
            .check()
            .unwrap();
        let counts: Vec<i64> = counts.take(0).unwrap();
        assert_eq!(counts, vec![1]);
        assert_eq!(markers(&db, "fixture_uuid").await.len(), 1);
        assert!(
            db.query("RETURN fn::store::exists(fixture_other:one);")
                .await
                .unwrap()
                .check()
                .is_err()
        );
        assert!(
            db.query("CREATE fixture_marker:bad SET state='invalid';")
                .await
                .unwrap()
                .check()
                .is_err()
        );
        assert_eq!(markers(&db, "fixture_marker").await.len(), 1);
    })
    .await
    .expect("current native schema profile exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_private_function_effects_share_the_caller_transaction() {
    const SQL: &str = "DEFINE TABLE fixture_marker SCHEMAFULL; DEFINE FIELD value ON fixture_marker TYPE string; CREATE fixture_marker:base SET value='base'; DEFINE FUNCTION fn::store::write($id: record<fixture_marker>, $value: string) -> bool { UPDATE ONLY $id SET value=$value RETURN NONE; RETURN true; }; DEFINE FUNCTION fn::store::outer($id: record<fixture_marker>, $value: string) -> bool { RETURN fn::store::write($id,$value); };";
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, db, _other) = fixture().await;
        let base = module("store", "fixture_marker", ModuleLayer::Kernel, vec![migration(0, "base", SQL, vec![])], vec![]);
        let owner = ModuleSetup::builder(base.name().clone(), base.layer())
            .ownership(vec![OwnershipClaim::Table(TableName::new("fixture_marker").unwrap()), OwnershipClaim::FunctionPrefix(FunctionPrefix::new("fn::store::").unwrap())])
            .execution(base.execution().clone()).lane(base.lane().clone()).build().unwrap();
        let baseline = ModuleRegistry::new(vec![owner.clone()]).unwrap();
        prepare(baseline.select(vec![]).unwrap()).unwrap().apply(&db).await.unwrap();
        for (sql, succeeds) in [
            ("fn::store::outer(fixture_marker:base,'changed'); CREATE fixture_marker:side SET value='side'; THROW 'controlled failure';", false),
            ("fn::store::outer(fixture_marker:base,'changed'); CREATE fixture_marker:side SET value='side';", true),
        ] {
            let updated = ModuleSetup::builder(owner.name().clone(), owner.layer()).ownership(owner.ownership().to_vec()).execution(owner.execution().clone())
                .lane(MigrationLane::new(vec![migration(0,"base",SQL,vec![]), migration(1,"settle",sql,vec![])]).unwrap()).build().unwrap();
            let catalog = ModuleRegistry::new(vec![updated]).unwrap();
            let prepared = prepare(catalog.select(vec![]).unwrap()).unwrap();
            assert_eq!(prepared.apply(&db).await.is_ok(), succeeds);
            let mut rows = db.query("SELECT VALUE value FROM ONLY fixture_marker:base;").await.unwrap().check().unwrap();
            let value: Option<String> = rows.take(0).unwrap();
            assert_eq!(value.as_deref(), Some(if succeeds { "changed" } else { "base" }));
            assert_eq!(markers(&db,"fixture_marker").await.len(), if succeeds {2} else {1});
            assert_eq!(prepared.status(&db).await.unwrap().lane(&name("store")).unwrap().current, Some(MigrationVersion::new(if succeeds {1} else {0})));
        }
    }).await.expect("native private function transaction exceeded 120 seconds");
}
