//! Fresh selected schema lanes, disabled-owner absence and reconnect admission.
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use surrealdb::types::SurrealValue;
use veoveo_modules::*;
use veoveo_platform_store::{PlatformStore, PrincipalKind, StoreError};
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;

fn optional_modules() -> Vec<ModuleSetup> {
    let execution = |name| fixture::module_lanes::execution(name).unwrap();
    vec![
        veoveo_reason_mcp::schema::module_setup(execution("reason")).unwrap(),
        veoveo_stream_mcp::schema::module_setup(execution("stream")).unwrap(),
        veoveo_agent_runtime::schema::module_setup(execution("agents")).unwrap(),
        veoveo_workspace::schema::module_setup(execution("workspace")).unwrap(),
        veoveo_recording_mcp::schema::module_setup(execution("recordings")).unwrap(),
        veoveo_map_mcp::schema::module_setup(execution("map")).unwrap(),
        veoveo_computers::schema::module_setup(execution("computers")).unwrap(),
        veoveo_time_mcp::schema::module_setup(execution("time")).unwrap(),
        veoveo_uav_sim_mcp::schema::module_setup(execution("uav")).unwrap(),
        veoveo_frames_mcp::schema::module_setup(execution("frames")).unwrap(),
        veoveo_media_mcp::schema::module_setup(execution("media")).unwrap(),
        veoveo_optimization_mcp::schema::module_setup(execution("optimization")).unwrap(),
    ]
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, SurrealValue)]
struct Inventory {
    tables: BTreeMap<String, String>,
    functions: BTreeMap<String, String>,
    analyzers: BTreeMap<String, String>,
}
async fn inventory(store: &PlatformStore) -> Inventory {
    store
        .client()
        .query(include_str!("queries/schema_lanes/inventory.surql"))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take::<Option<Inventory>>(0)
        .unwrap()
        .unwrap()
}
/// SurrealDB's complete per-table DDL maps. Field definitions include types,
/// assertions and record-reference policies; no field or child map is projected away.
#[derive(Clone, Debug, PartialEq, serde::Serialize, SurrealValue)]
struct TableSchema {
    fields: BTreeMap<String, String>,
    indexes: BTreeMap<String, String>,
    events: BTreeMap<String, String>,
    tables: BTreeMap<String, String>,
    lives: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq)]
struct InstalledSchema {
    database: Inventory,
    tables: BTreeMap<TableName, TableSchema>,
    ownership: BTreeMap<TableName, ModuleName>,
}
async fn capture_installed_schema(
    store: &PlatformStore,
    registry: &ModuleRegistry,
) -> InstalledSchema {
    let current = inventory(store).await;
    let mut tables = BTreeMap::new();
    let mut ownership = BTreeMap::new();
    for name in current.tables.keys() {
        let table = TableName::new(name).expect("installed table name");
        let owner = registry
            .owner_of_table(&table)
            .expect("installed table owner");
        let info = store
            .client()
            .query(include_str!(
                "../../../../testing/fixtures/queries/store/table_info.surql"
            ))
            .bind(("table", table.as_str().to_owned()))
            .await
            .unwrap()
            .check()
            .unwrap()
            .take::<Option<TableSchema>>(0)
            .unwrap()
            .expect("installed table schema");
        // A fresh schema has no instance-specific LIVE subscriptions. Keep this
        // map in equality as well, rather than silently removing unexpected lives.
        assert!(
            info.lives.is_empty(),
            "fresh table {table} has LIVE subscriptions"
        );
        ownership.insert(table.clone(), owner.name().clone());
        tables.insert(table, info);
    }
    assert_eq!(tables.len(), current.tables.len());
    assert_eq!(ownership.len(), tables.len());
    InstalledSchema {
        database: current,
        tables,
        ownership,
    }
}
/// Optional diagnostic output from this test's owned fresh fixture; no runtime credentials.
fn write_schema_snapshot(schema: &InstalledSchema) {
    if let Some(output) = std::env::var_os("VEOVEO_TEST_SCHEMA_INFO_SNAPSHOT") {
        // Names become text only at this optional JSON diagnostic boundary.
        let tables = schema
            .tables
            .iter()
            .map(|(name, body)| (name.as_str(), body))
            .collect::<BTreeMap<_, _>>();
        let ownership = schema
            .ownership
            .iter()
            .map(|(table, owner)| (table.as_str(), owner.as_str()))
            .collect::<BTreeMap<_, _>>();
        std::fs::write(output, serde_json::to_vec_pretty(&serde_json::json!({"database":schema.database,"tables":tables,"ownership":ownership})).unwrap()).unwrap();
    }
}
fn assert_other_member_drift_is_detected(expected: &InstalledSchema) {
    let mut changed = expected.clone();
    changed
        .tables
        .values_mut()
        .flat_map(|table| table.events.values_mut())
        .next()
        .expect("installed event")
        .push_str(" /* changed event */");
    assert_eq!(changed.database, expected.database);
    assert_eq!(changed.ownership, expected.ownership);
    assert_eq!(changed.tables.len(), expected.tables.len());
    assert_ne!(
        changed, *expected,
        "event definition drift was not detected"
    );

    let mut changed = expected.clone();
    changed
        .database
        .functions
        .values_mut()
        .next()
        .expect("installed function")
        .push_str(" /* changed function */");
    assert_eq!(
        changed.database.functions.len(),
        expected.database.functions.len()
    );
    assert_eq!(changed.tables, expected.tables);
    assert_eq!(changed.ownership, expected.ownership);
    assert_ne!(
        changed, *expected,
        "function definition drift was not detected"
    );

    let mut changed = expected.clone();
    changed
        .database
        .analyzers
        .values_mut()
        .next()
        .expect("installed analyzer")
        .push_str(" /* changed analyzer */");
    assert_eq!(
        changed.database.analyzers.len(),
        expected.database.analyzers.len()
    );
    assert_eq!(changed.tables, expected.tables);
    assert_eq!(changed.ownership, expected.ownership);
    assert_ne!(
        changed, *expected,
        "analyzer definition drift was not detected"
    );

    let mut changed = expected.clone();
    let table = TableName::new("reason_analysis").unwrap();
    assert_eq!(
        changed.ownership[&table],
        ModuleName::new("reason").unwrap()
    );
    changed
        .ownership
        .insert(table, ModuleName::new("store").unwrap());
    assert_eq!(changed.ownership.len(), expected.ownership.len());
    assert_eq!(changed.database, expected.database);
    assert_eq!(changed.tables, expected.tables);
    assert_ne!(changed, *expected, "table ownership drift was not detected");
}
async fn compare_independent_fresh_schema(
    first: &PlatformStore,
    registry: &ModuleRegistry,
    prepared: &runner::PreparedInstallation<'_>,
) {
    let expected = capture_installed_schema(first, registry).await;
    // TestDb allocates a distinct database and owned container, not another
    // connection to the first installation or a replay of its applied receipts.
    let second = fixture::TestDb::new().await;
    assert_ne!(first.config().database(), second.a.config().database());
    assert_ne!(first.config().endpoint(), second.a.config().endpoint());
    let admin = second.admin().await;
    prepared.apply(admin.client()).await.unwrap();
    let actual = capture_installed_schema(&second.a, registry).await;
    assert_eq!(
        actual, expected,
        "independently fresh composed schemas differ"
    );
    assert!(!actual.database.functions.is_empty());
    assert!(!actual.database.analyzers.is_empty());
    for marker in [" TYPE ", " ASSERT ", " REFERENCE "] {
        assert!(
            actual
                .tables
                .values()
                .flat_map(|table| table.fields.values())
                .any(|definition| definition.contains(marker)),
            "missing captured {marker} definitions"
        );
    }
    assert!(
        actual
            .tables
            .values()
            .any(|table| !table.indexes.is_empty())
    );
    assert!(actual.tables.values().any(|table| !table.events.is_empty()));
    write_schema_snapshot(&expected);
    assert_other_member_drift_is_detected(&expected);
    // Real schema edits in only the second disposable installation demonstrate
    // comparison of child definitions, while the outer object inventory stays equal.
    let mut previous = actual;
    let reason = TableName::new("reason_analysis").unwrap();
    for (index, mutation) in [
        include_str!("queries/schema_lanes/changed_field.surql"),
        include_str!("queries/schema_lanes/missing_index.surql"),
    ]
    .into_iter()
    .enumerate()
    {
        admin
            .client()
            .query(mutation)
            .await
            .unwrap()
            .check()
            .unwrap();
        let changed = capture_installed_schema(&second.a, registry).await;
        assert_eq!(changed.database, expected.database);
        assert_eq!(changed.ownership, expected.ownership);
        assert_eq!(changed.tables.len(), expected.tables.len());
        if index == 0 {
            assert_eq!(
                changed.tables[&reason].fields.len(),
                previous.tables[&reason].fields.len()
            );
            assert_ne!(
                changed.tables[&reason].fields["task_type"],
                previous.tables[&reason].fields["task_type"]
            );
            assert_eq!(
                changed.tables[&reason].indexes,
                previous.tables[&reason].indexes
            );
        } else {
            assert_eq!(
                changed.tables[&reason].fields,
                previous.tables[&reason].fields
            );
            assert!(
                previous.tables[&reason]
                    .indexes
                    .contains_key("reason_analysis_task")
            );
            assert!(
                !changed.tables[&reason]
                    .indexes
                    .contains_key("reason_analysis_task")
            );
            assert_eq!(
                changed.tables[&reason].indexes.len() + 1,
                previous.tables[&reason].indexes.len()
            );
        }
        assert_ne!(changed, previous, "child schema drift was not detected");
        previous = changed;
    }
}

async fn receipt_count(store: &PlatformStore) -> usize {
    store
        .client()
        .query(include_str!("queries/schema_lanes/receipts.surql"))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take::<Vec<surrealdb::types::RecordId>>(0)
        .unwrap()
        .len()
}
async fn initial_state(store: &PlatformStore, map: bool) -> Vec<serde_json::Value> {
    let mut response = store
        .client()
        .query(include_str!("queries/schema_lanes/initial_state.surql"))
        .await
        .unwrap()
        .check()
        .unwrap();
    let mut state = (0..2)
        .map(|index| {
            response
                .take::<Option<surrealdb::types::Value>>(index)
                .unwrap()
                .expect("owner singleton initialized")
                .into_json_value()
        })
        .collect::<Vec<_>>();
    if map {
        state.push(
            store
                .client()
                .query(include_str!("queries/schema_lanes/map_initial_state.surql"))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take::<Option<surrealdb::types::Value>>(0)
                .unwrap()
                .expect("Map projection head initialized")
                .into_json_value(),
        );
    }
    state
}
#[tokio::test]
async fn fresh_selected_lanes_exclude_disabled_owners_and_replay_without_reapplying() {
    tokio::time::timeout(Duration::from_secs(900), async {
        for requested in [
            None,
            Some("agents"),
            Some("workspace"),
            Some("map"),
            Some("recordings"),
            Some("all"),
        ] {
            // Admit the complete catalog/selection before any disposable database effects.
            let registry = fixture::module_lanes::registry(optional_modules()).unwrap();
            let requested = if requested == Some("all") {
                registry
                    .modules()
                    .iter()
                    .filter(|owner| owner.layer() == ModuleLayer::Optional)
                    .map(|owner| owner.name().clone())
                    .collect()
            } else {
                requested
                    .map(|name| ModuleName::new(name).unwrap())
                    .into_iter()
                    .collect()
            };
            let selection = registry.select(requested).unwrap();
            let prepared = runner::prepare(selection).unwrap();
            let db = fixture::TestDb::new().await;
            let admin = db.admin().await;
            prepared.apply(admin.client()).await.unwrap();
            let status = prepared.status(db.a.client()).await.unwrap();
            assert!(status.is_current(), "{status:?}");
            let current = inventory(&db.a).await;
            for table in current.tables.keys() {
                let table = TableName::new(table).unwrap();
                let owner = registry
                    .owner_of_table(&table)
                    .expect("every current table has an owner");
                assert!(
                    status
                        .lanes
                        .iter()
                        .any(|lane| lane.module == *owner.name() && lane.selected),
                    "disabled owner table {table}"
                );
            }
            for function in current.functions.keys() {
                let function = FunctionName::new(if function.starts_with("fn::") {
                    function.clone()
                } else {
                    format!("fn::{function}")
                })
                .unwrap();
                let owner = registry
                    .owner_of_function(&function)
                    .expect("every current function has an owner");
                assert!(
                    status
                        .lanes
                        .iter()
                        .any(|lane| lane.module == *owner.name() && lane.selected),
                    "disabled owner function {function}"
                );
            }
            for analyzer in current.analyzers.keys() {
                let analyzer = AnalyzerName::new(analyzer).unwrap();
                let owner = registry
                    .owner_of_analyzer(&analyzer)
                    .expect("every current analyzer has an owner");
                assert!(
                    status
                        .lanes
                        .iter()
                        .any(|lane| lane.module == *owner.name() && lane.selected)
                );
            }
            assert!(!current.tables.contains_key("platform_schema_migration"));
            assert!(!current.tables.contains_key("platform_downstream_migration"));
            for (owner, table) in [
                ("agents", "agent_definition"),
                ("workspace", "workspace_chat"),
                ("map", "map_feature_layer"),
                ("recordings", "recording"),
                ("reason", "reason_analysis"),
                ("stream", "stream_run"),
                ("uav", "uav_task"),
            ] {
                let selected = status
                    .lanes
                    .iter()
                    .any(|lane| lane.module.as_str() == owner && lane.selected);
                assert_eq!(current.tables.contains_key(table), selected, "{owner}");
            }
            if status
                .lanes
                .iter()
                .any(|lane| lane.module.as_str() == "agents" && lane.selected)
                && !status
                    .lanes
                    .iter()
                    .any(|lane| lane.module.as_str() == "workspace" && lane.selected)
            {
                assert!(!current.tables.contains_key("workspace_agent"));
            }
            let seeds =
                initial_state(&db.a, current.tables.contains_key("map_projection_state")).await;
            assert_eq!(seeds[0]["inflight_bytes"], 0);
            assert_eq!(seeds[1]["generation"], 0);
            assert_eq!(seeds[1]["cursor"], 0);
            assert!(seeds[1]["owner"].is_string());
            if seeds.len() == 3 {
                assert_eq!(seeds[2]["last_sequence"], 0);
            }
            if status.lanes.iter().filter(|lane| lane.selected).count() == registry.modules().len()
            {
                compare_independent_fresh_schema(&db.a, &registry, &prepared).await;
                let originals = current
                    .tables
                    .iter()
                    .filter_map(|(table, definition)| {
                        definition
                            .contains("INCLUDE ORIGINAL")
                            .then_some(table.as_str())
                    })
                    .collect::<BTreeSet<_>>();
                assert_eq!(
                    originals,
                    BTreeSet::from([
                        "reason_analysis",
                        "stream_run",
                        "map_travel_model_task",
                        "principal",
                        "task",
                        "artifact_blob",
                        "artifact_occurrence",
                        "artifact_grant",
                        "artifact_access_request",
                        "share_link",
                        "agent",
                        "wake",
                        "recording",
                        "recording_layer",
                        "recording_manifest_publication",
                        "computer_automation_grant",
                        "computer_session_grant",
                        "computer_cli_grant",
                        "computer_maintenance",
                    ])
                );
            }
            let identity =
                db.a.ensure_identity(
                    "reconnect",
                    "reader",
                    "https://fixture.invalid",
                    "reader",
                    PrincipalKind::User,
                )
                .await
                .unwrap();
            let count = receipt_count(&db.a).await;
            prepared.apply(admin.client()).await.unwrap();
            assert_eq!(receipt_count(&db.a).await, count);
            assert_eq!(inventory(&db.a).await, current);
            assert_eq!(initial_state(&db.a, seeds.len() == 3).await, seeds);
            let reconnect = db.connect_at(db.a.config().endpoint().as_str()).await;
            let recovered = reconnect
                .ensure_identity(
                    "reconnect",
                    "reader",
                    "https://fixture.invalid",
                    "reader",
                    PrincipalKind::User,
                )
                .await
                .unwrap();
            assert_eq!(recovered.tenant_id, identity.tenant_id);
            assert_eq!(recovered.principal_id, identity.principal_id);
            assert_eq!(recovered.tenant_key, identity.tenant_key);
            assert_eq!(recovered.principal_key, identity.principal_key);
        }
    })
    .await
    .expect("fresh schema matrix exceeded 900 seconds");
}
#[tokio::test]
async fn missing_prerequisites_reject_without_writes_and_old_history_requires_fresh_install() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let before = inventory(&db.a).await;
        let missing = fixture::module_lanes::registry(vec![
            veoveo_workspace::schema::module_setup(
                fixture::module_lanes::execution("workspace").unwrap(),
            )
            .unwrap(),
        ]);
        assert!(missing.is_err());
        assert_eq!(inventory(&db.a).await, before);
        let admin = db.admin().await;
        for (define, remove) in [
            (
                include_str!("queries/schema_lanes/define_mixed_history.surql"),
                include_str!("queries/schema_lanes/remove_mixed_history.surql"),
            ),
            (
                include_str!("queries/schema_lanes/define_downstream_history.surql"),
                include_str!("queries/schema_lanes/remove_downstream_history.surql"),
            ),
        ] {
            admin.client().query(define).await.unwrap().check().unwrap();
            let rejection = PlatformStore::connect(db.a.config().clone())
                .await
                .unwrap_err();
            assert!(matches!(rejection, StoreError::FreshInstallationRequired));
            assert!(rejection.to_string().contains("fresh installation"));
            admin.client().query(remove).await.unwrap().check().unwrap();
        }
        assert_eq!(inventory(&db.a).await, before);
    })
    .await
    .expect("fresh-install rejection controls exceeded 180 seconds");
}
