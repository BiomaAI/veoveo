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

const BASE: &str = include_str!("../../queries/tests/executor/fixtures/base.surql");
const ADVANCE: &str = include_str!("../../queries/tests/executor/fixtures/advance.surql");
const FEATURE: &str = include_str!("../../queries/tests/executor/fixtures/feature.surql");

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
                            db.query(include_str!("../../queries/tests/executor/fixture/define_namespace_if_not.surql"))
                                .await
                                .expect("fixture namespace definition")
                                .check()
                                .expect("fixture namespace admission");
                            db.use_ns("module_runner")
                                .await
                                .expect("fixture namespace selection");
                            db.query(include_str!("../../queries/tests/executor/fixture/define_database_if_not.surql"))
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
        .query(include_str!(
            "../../queries/tests/executor/markers/select_value_id_from.surql"
        ))
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
        let failure = registry(vec![first.clone(), migration(1, "fail", include_str!("../../queries/tests/executor/native_failed_body_rolls_back_and_unknown_history_blocks_pending_effects/create_fixture_marker_rolled_back_set.surql"), vec![])], vec![]);
        let prepared = prepare(failure.select(vec![]).unwrap()).unwrap();
        let error = prepared.apply(&a).await.unwrap_err();
        assert!(!format!("{error:?} {error}").contains("private-token"));
        assert!(error.to_string().contains("migration body transaction failed"));
        assert!(error.to_string().contains("Thrown"));
        assert!(error.to_string().contains("statement "));
        assert_eq!(markers(&a, "fixture_marker").await.len(), 1);
        assert_eq!(prepared.status(&a).await.unwrap().lane(&name("store")).unwrap().current, Some(MigrationVersion::new(0)));
        a.query(include_str!("../../queries/tests/executor/native_failed_body_rolls_back_and_unknown_history_blocks_pending_effects/create_platform_module_lane_unknown_set.surql")).await.unwrap().check().unwrap();
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
        a.query(include_str!("../../queries/tests/executor/native_incompatible_or_uncommitted_infrastructure_is_not_a_winner/define_table_platform_module_lane_schemaless.surql")).await.unwrap().check().unwrap();
        assert!(prepared.apply(&a).await.is_err());
        let mut response = a.query(include_str!("../../queries/tests/executor/native_incompatible_or_uncommitted_infrastructure_is_not_a_winner/info_for_db.surql")).await.unwrap();
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
            include_str!("../../queries/tests/executor/native_transaction_timeout_and_dropped_awaiter_cancel_before_commit/create_fixture_marker_cancelled_set.surql"),
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
        b.query(include_str!("../../queries/tests/executor/native_preparation_generation_fences_delayed_rotation_and_conflicting_identity/return_true.surql")).await.unwrap().check().unwrap();
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
                vec![migration(0, "initial", include_str!("../../queries/tests/executor/native_reciprocal_kernel_field_types_allow_forward_definition_and_check_values/define_table_fixture_left_schemafull.surql"), vec![])],
                vec![],
            );
            let right = module(
                "right",
                "fixture_right",
                ModuleLayer::Kernel,
                vec![migration(0, "initial", include_str!("../../queries/tests/executor/native_reciprocal_kernel_field_types_allow_forward_definition_and_check_values/define_table_fixture_right_schemafull.surql"), vec![])],
                vec![],
            );
            let registry = ModuleRegistry::new(if reverse { vec![right, left] } else { vec![left, right] }).unwrap();
            let selection = registry.select(vec![]).unwrap();
            assert_eq!(selection.ordered()[0].name().as_str(), if reverse { "right" } else { "left" });
            prepare(selection).unwrap().apply(&db).await.unwrap();
            db.query(include_str!("../../queries/tests/executor/native_reciprocal_kernel_field_types_allow_forward_definition_and_check_values/create_fixture_left_one_set.surql"))
                .await.unwrap().check().unwrap();
            assert_eq!(markers(&db, "fixture_left").await.len(), 1);
            assert_eq!(markers(&db, "fixture_right").await.len(), 1);
            assert!(db.query(include_str!("../../queries/tests/executor/native_reciprocal_kernel_field_types_allow_forward_definition_and_check_values/create_fixture_left_wrong_set.surql")).await.unwrap().check().is_err());
            assert!(db.query(include_str!("../../queries/tests/executor/native_reciprocal_kernel_field_types_allow_forward_definition_and_check_values/create_fixture_right_wrong_set.surql")).await.unwrap().check().is_err());
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
        let mut before = db.query(include_str!("../../queries/tests/executor/native_incompatible_or_uncommitted_infrastructure_is_not_a_winner/info_for_db.surql")).await.unwrap().check().unwrap();
        let before: surrealdb::types::Value = before.take(0).unwrap();
        let registry = ModuleRegistry::new(vec![
            module("a_valid", "fixture_left", ModuleLayer::Kernel, vec![migration(0, "initial", include_str!("../../queries/tests/executor/native_invalid_field_link_selection_has_no_bookkeeping_or_owner_effects/define_table_fixture_left_create.surql"), vec![])], vec![]),
            module("z_invalid", "fixture_right", ModuleLayer::Kernel, vec![migration(0, "initial", include_str!("../../queries/tests/executor/native_invalid_field_link_selection_has_no_bookkeeping_or_owner_effects/define_table_fixture_right_define.surql"), vec![])], vec![]),
        ]).unwrap();
        assert!(prepare(registry.select(vec![]).unwrap()).is_err());
        let mut after = db.query(include_str!("../../queries/tests/executor/native_incompatible_or_uncommitted_infrastructure_is_not_a_winner/info_for_db.surql")).await.unwrap().check().unwrap();
        let after: surrealdb::types::Value = after.take(0).unwrap();
        assert_eq!(before, after);
    }).await.expect("invalid field selection preflight exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_owned_update_api_and_caller_settle_or_roll_back_in_one_transaction() {
    const API: &str = include_str!(
        "../../queries/tests/executor/native_owned_update_api_and_caller_settle_or_roll_back_in_one_transaction/api.surql"
    );
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, db, _other) = fixture().await;
        let sql = include_str!("../../queries/tests/executor/native_owned_update_api_and_caller_settle_or_roll_back_in_one_transaction/initial.surql");
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
            vec![migration(0, "settle", include_str!("../../queries/tests/executor/native_owned_update_api_and_caller_settle_or_roll_back_in_one_transaction/settle_failed.surql"), vec![prerequisite.clone()])], vec![prerequisite.clone()]);
        let registry = ModuleRegistry::new(vec![owner.clone(), failed]).unwrap();
        let prepared = prepare(registry.select(vec![name("consumer")]).unwrap()).unwrap();
        assert!(prepared.apply(&db).await.is_err());
        let mut values = db.query(include_str!("../../queries/tests/executor/native_owned_update_api_and_caller_settle_or_roll_back_in_one_transaction/select_value_value_from.surql")).await.unwrap().check().unwrap();
        let values: Vec<String> = values.take(0).unwrap();
        assert_eq!(values, vec!["base"]);
        let absent = db.query(include_str!("../../queries/tests/executor/native_owned_update_api_and_caller_settle_or_roll_back_in_one_transaction/select_from_fixture_optional.surql")).await.unwrap().check().unwrap_err();
        assert!(absent.to_string().contains("does not exist"));
        assert_eq!(prepared.status(&db).await.unwrap().lane(&name("consumer")).unwrap().current, None);
        let successful = module("consumer", "fixture_optional", ModuleLayer::Optional,
            vec![migration(0, "settle", include_str!("../../queries/tests/executor/native_owned_update_api_and_caller_settle_or_roll_back_in_one_transaction/settle_success.surql"), vec![prerequisite.clone()])], vec![prerequisite]);
        let registry = ModuleRegistry::new(vec![owner, successful]).unwrap();
        let prepared = prepare(registry.select(vec![name("consumer")]).unwrap()).unwrap();
        prepared.apply(&db).await.unwrap();
        let mut values = db.query(include_str!("../../queries/tests/executor/native_owned_update_api_and_caller_settle_or_roll_back_in_one_transaction/select_value_value_from.surql")).await.unwrap().check().unwrap();
        let values: Vec<String> = values.take(0).unwrap();
        assert_eq!(values, vec!["released"]);
        assert_eq!(markers(&db, "fixture_optional").await.len(), 1);
        assert_eq!(prepared.status(&db).await.unwrap().lane(&name("consumer")).unwrap().current, Some(MigrationVersion::new(0)));
    }).await.expect("owned update API transaction qualification exceeded 120 seconds");
}

#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn native_current_schema_values_views_and_guarded_events() {
    const SQL: &str = include_str!(
        "../../queries/tests/executor/native_current_schema_values_views_and_guarded_events/sql.surql"
    );
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
            .query(include_str!("../../queries/tests/executor/native_current_schema_values_views_and_guarded_events/select_value_tags_from.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        let tags: Vec<Vec<String>> = tags.take(0).unwrap();
        assert_eq!(tags, vec![vec!["pin"]]);
        let mut head = db
            .query(include_str!("../../queries/tests/executor/native_current_schema_values_views_and_guarded_events/select_value_last_sequence_from.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        let head: Vec<i64> = head.take(0).unwrap();
        assert_eq!(head, vec![1]);
        let mut counts = db
            .query(include_str!("../../queries/tests/executor/native_current_schema_values_views_and_guarded_events/select_value_count_from.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
        let counts: Vec<i64> = counts.take(0).unwrap();
        assert_eq!(counts, vec![1]);
        assert_eq!(markers(&db, "fixture_uuid").await.len(), 1);
        assert!(
            db.query(include_str!("../../queries/tests/executor/native_current_schema_values_views_and_guarded_events/return_fn_store_exists.surql"))
                .await
                .unwrap()
                .check()
                .is_err()
        );
        assert!(
            db.query(include_str!("../../queries/tests/executor/native_current_schema_values_views_and_guarded_events/create_fixture_marker_bad_set.surql"))
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
    const SQL: &str = include_str!(
        "../../queries/tests/executor/native_private_function_effects_share_the_caller_transaction/sql.surql"
    );
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, db, _other) = fixture().await;
        let base = module("store", "fixture_marker", ModuleLayer::Kernel, vec![migration(0, "base", SQL, vec![])], vec![]);
        let owner = ModuleSetup::builder(base.name().clone(), base.layer())
            .ownership(vec![OwnershipClaim::Table(TableName::new("fixture_marker").unwrap()), OwnershipClaim::FunctionPrefix(FunctionPrefix::new("fn::store::").unwrap())])
            .execution(base.execution().clone()).lane(base.lane().clone()).build().unwrap();
        let baseline = ModuleRegistry::new(vec![owner.clone()]).unwrap();
        prepare(baseline.select(vec![]).unwrap()).unwrap().apply(&db).await.unwrap();
        for (sql, succeeds) in [
            (include_str!("../../queries/tests/executor/native_private_function_effects_share_the_caller_transaction/fn_store_outer_fixture_marker.surql"), false),
            (include_str!("../../queries/tests/executor/native_private_function_effects_share_the_caller_transaction/fn_store_outer_fixture_marker_2.surql"), true),
        ] {
            let updated = ModuleSetup::builder(owner.name().clone(), owner.layer()).ownership(owner.ownership().to_vec()).execution(owner.execution().clone())
                .lane(MigrationLane::new(vec![migration(0,"base",SQL,vec![]), migration(1,"settle",sql,vec![])]).unwrap()).build().unwrap();
            let catalog = ModuleRegistry::new(vec![updated]).unwrap();
            let prepared = prepare(catalog.select(vec![]).unwrap()).unwrap();
            assert_eq!(prepared.apply(&db).await.is_ok(), succeeds);
            let mut rows = db.query(include_str!("../../queries/tests/executor/native_private_function_effects_share_the_caller_transaction/select_value_value_from.surql")).await.unwrap().check().unwrap();
            let value: Option<String> = rows.take(0).unwrap();
            assert_eq!(value.as_deref(), Some(if succeeds { "changed" } else { "base" }));
            assert_eq!(markers(&db,"fixture_marker").await.len(), if succeeds {2} else {1});
            assert_eq!(prepared.status(&db).await.unwrap().lane(&name("store")).unwrap().current, Some(MigrationVersion::new(if succeeds {1} else {0})));
        }
    }).await.expect("native private function transaction exceeded 120 seconds");
}

fn runtime_plan(catalog: &ModuleRegistry, enabled: Vec<ModuleName>) -> ModulePlanDocument {
    ModulePlanDocument::generate(
        catalog,
        &ModuleSelectionDocument::new(
            enabled,
            "1".parse().unwrap(),
            CredentialRevision::new("fixture-v1").unwrap(),
        )
        .unwrap(),
        CompositionIdentity::new(format!("sha256:{}", "a".repeat(64))).unwrap(),
        vec![],
    )
    .unwrap()
}
#[test]
fn runtime_prerequisites_match_only_the_required_compiled_closure() {
    let catalog = registry(
        vec![migration(0, "base", BASE, vec![])],
        vec![migration(0, "feature", FEATURE, vec![])],
    );
    let selected = runtime_plan(&catalog, vec![name("feature")]);
    assert!(
        crate::runner::RuntimePrerequisites::new(&catalog, &selected, &[name("store")]).is_ok()
    );
    assert!(
        crate::runner::RuntimePrerequisites::new(&catalog, &selected, &[name("feature")]).is_ok()
    );
    assert!(
        crate::runner::RuntimePrerequisites::new(&catalog, &selected, &[name("unknown")]).is_err()
    );
    assert!(crate::runner::RuntimePrerequisites::new(&catalog, &selected, &[]).is_err());
    let dependency_drift = ModuleRegistry::new(vec![
        catalog.module(&name("store")).unwrap().clone(),
        module(
            "feature",
            "fixture_optional",
            ModuleLayer::Optional,
            vec![migration(0, "feature", FEATURE, vec![])],
            vec![LaneRequirement::AtLeast {
                module: name("store"),
                version: MigrationVersion::new(0),
            }],
        ),
    ])
    .unwrap();
    assert!(
        crate::runner::RuntimePrerequisites::new(&dependency_drift, &selected, &[name("feature")])
            .is_err()
    );
    let kernels = runtime_plan(&catalog, vec![]);
    assert!(
        crate::runner::RuntimePrerequisites::new(&catalog, &kernels, &[name("feature")]).is_err()
    );
    let mut document = serde_json::to_value(&selected).unwrap();
    document["lanes"][0]["laneSha256"] = serde_json::json!(format!("sha256:{}", "b".repeat(64)));
    let drift = serde_json::from_value(document).unwrap();
    assert!(
        crate::runner::RuntimePrerequisites::new(&catalog, &drift, &[name("feature")]).is_err()
    );
    assert_ne!(
        crate::preparation_key(&selected, "runtime-a").unwrap(),
        crate::preparation_key(&selected, "runtime-b").unwrap()
    );
}

#[tokio::test]
#[ignore = "requires locally available pinned SurrealDB fixture"]
async fn native_runtime_prerequisites_require_preparation_and_scoped_complete_histories() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let (_fixture, a, _) = fixture().await;
        let catalog = registry(
            vec![migration(0, "base", BASE, vec![])],
            vec![migration(
                0,
                "feature",
                FEATURE,
                vec![LaneRequirement::Satisfied(name("store"))],
            )],
        );
        let selected = runtime_plan(&catalog, vec![name("feature")]);
        let key = crate::preparation_key(&selected, "fixture-runtime").unwrap();
        let readiness =
            crate::runner::RuntimePrerequisites::new(&catalog, &selected, &[name("store")])
                .unwrap();
        let prepared = prepare(catalog.select(vec![name("feature")]).unwrap()).unwrap();
        prepared.initialize(&a).await.unwrap();
        assert!(
            readiness
                .require(&a, &key, Default::default())
                .await
                .is_err(),
            "absent preparation"
        );
        prepared.claim_preparation(&a, &key).await.unwrap();
        assert!(
            readiness
                .require(&a, &key, Default::default())
                .await
                .is_err(),
            "partial preparation"
        );
        prepared
            .complete_preparation(
                &a,
                &key,
                crate::runner::DatabaseEditorCredentials::new(
                    "fixture-runtime",
                    "fixture-password",
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            readiness
                .require(&a, &key, Default::default())
                .await
                .is_err(),
            "missing required lane histories"
        );
        prepared.apply(&a).await.unwrap();
        readiness
            .require(&a, &key, Default::default())
            .await
            .unwrap();
        let wrong = crate::preparation_key(&selected, "different-runtime").unwrap();
        assert!(
            readiness
                .require(&a, &wrong, Default::default())
                .await
                .is_err()
        );
        // Full status keeps rejecting unknown histories; the runtime port is explicitly scoped.
        let partial =
            ModuleRegistry::new(vec![catalog.module(&name("store")).unwrap().clone()]).unwrap();
        assert!(
            prepare(partial.select(vec![]).unwrap())
                .unwrap()
                .status(&a)
                .await
                .is_err()
        );
        a.query(include_str!(
            "../../queries/tests/executor/runtime_history_drift.surql"
        ))
        .bind(("module", "feature".to_owned()))
        .await
        .unwrap()
        .check()
        .unwrap();
        readiness
            .require(&a, &key, Default::default())
            .await
            .unwrap();
        assert!(
            prepared.status(&a).await.is_err(),
            "full registry still checks unrelated history"
        );
        a.query(include_str!(
            "../../queries/tests/executor/runtime_history_drift.surql"
        ))
        .bind(("module", "store".to_owned()))
        .await
        .unwrap()
        .check()
        .unwrap();
        assert!(
            readiness
                .require(&a, &key, Default::default())
                .await
                .is_err(),
            "required checksum drift"
        );
    })
    .await
    .expect("runtime prerequisite fixture exceeded 120 seconds");
}

#[test]
fn conflict_retries_exhaust_and_never_admit_uncertain_or_refused_outcomes() {
    use transaction::Disposition;
    let zero = Duration::ZERO;
    assert_eq!(
        conflict_retry_delay(Disposition::AbortedCommitConflict, 1, zero),
        Some(Duration::from_millis(25))
    );
    assert_eq!(
        conflict_retry_delay(Disposition::AbortedCommitConflict, 15, zero),
        Some(Duration::from_millis(250))
    );
    assert_eq!(
        conflict_retry_delay(Disposition::AbortedCommitConflict, 16, zero),
        None
    );
    assert_eq!(
        conflict_retry_delay(
            Disposition::AbortedCommitConflict,
            1,
            Duration::from_secs(60)
        ),
        None
    );
    assert_eq!(
        conflict_retry_delay(
            Disposition::AbortedCommitConflict,
            1,
            Duration::from_millis(59_975)
        ),
        None
    );
    for attempt in 1..=16 {
        assert_eq!(
            conflict_retry_delay(Disposition::ObserveCommittedWinner, attempt, zero),
            None
        );
        assert_eq!(
            conflict_retry_delay(Disposition::Refused, attempt, zero),
            None
        );
    }
}
