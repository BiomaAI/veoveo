use std::time::Duration;

use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials, StoreError};

async fn fixture() -> PlatformStore {
    let endpoint = std::env::var("VEOVEO_SURREAL_ENDPOINT")
        .expect("an isolated SurrealDB endpoint is required");
    let config = StoreConfig::builder(
        endpoint,
        "veoveo_online_index_tests",
        format!("migration_{}", Uuid::now_v7().simple()),
        StoreCredentials::root("root", "root"),
    )
    .migrate_on_connect(true)
    .build()
    .unwrap();
    let store = PlatformStore::connect(config).await.unwrap();
    // Reconstruct the immediately preceding schema only in this disposable fixture.
    store.client().query(
        "REMOVE INDEX audit_event_resource_time ON audit_event; DELETE platform_schema_migration:72;",
    ).await.unwrap().check().unwrap();
    store
}

#[tokio::test]
#[ignore = "requires an isolated SurrealDB 3.2.4 endpoint"]
async fn populated_index_build_resumes_without_rewriting_migration_history() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let store = fixture().await;
        store.client().query(
            "FOR $n IN 0..10000 {
                CREATE audit_event SET action='online-index.fixture', resource_type='gateway_auth',
                    outcome='allowed', occurred_at=time::now();
            };
            DEFINE INDEX audit_event_resource_time ON audit_event FIELDS resource_type, occurred_at CONCURRENTLY;",
        ).await.unwrap().check().unwrap();
        assert_eq!(store.schema_status().await.unwrap().pending_versions, [72]);
        let (a, b) = tokio::join!(store.migrate(), store.migrate());
        assert!(a.unwrap().status.pending_versions.is_empty());
        assert!(b.unwrap().status.pending_versions.is_empty());
        let mut result = store.client().query(
            "INFO FOR INDEX audit_event_resource_time ON audit_event;
             SELECT count() AS count FROM audit_event GROUP ALL;
             SELECT VALUE id FROM audit_event WITH INDEX audit_event_resource_time
               WHERE resource_type='gateway_auth' LIMIT 1 EXPLAIN;",
        ).await.unwrap().check().unwrap();
        let info: Value = result.take(0).unwrap();
        assert!(format!("{info:?}").contains("ready"));
        #[derive(SurrealValue)]
        struct Count { count: u64 }
        let count: Vec<Count> = result.take(1).unwrap();
        assert_eq!(count[0].count, 10000);
        let plan: Value = result.take(2).unwrap();
        let plan = format!("{plan:?}");
        assert!(plan.contains("audit_event_resource_time") && !plan.contains("TableScan"), "{plan}");
        // validate_history checks the original published checksum, including after restart.
        assert!(store.migrate().await.unwrap().applied_versions.is_empty());
    }).await.unwrap();
}

#[tokio::test]
#[ignore = "requires an isolated SurrealDB 3.2.4 endpoint"]
async fn conflicting_index_definition_cannot_mark_migration_applied() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let store = fixture().await;
        store
            .client()
            .query("DEFINE INDEX audit_event_resource_time ON audit_event FIELDS occurred_at;")
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            store.migrate().await,
            Err(StoreError::MigrationPreparation { version: 72, .. })
        ));
        assert_eq!(store.schema_status().await.unwrap().pending_versions, [72]);
    })
    .await
    .unwrap();
}
