//! Isolated-store qualification of bounded retention and its actual query plan.
use chrono::{TimeDelta, Utc};
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_mcp_gateway::GatewayState;
use veoveo_platform_store::{
    GATEWAY_AUDIT_BATCH_LIMIT, PlatformStore, StoreConfig, StoreCredentials,
};

#[tokio::test]
#[ignore = "requires an isolated SurrealDB 3.2.4 endpoint"]
async fn audit_retention_uses_index_and_preserves_other_kinds_and_cutoff() {
    tokio::time::timeout(std::time::Duration::from_secs(60), qualify())
        .await
        .unwrap();
}

async fn qualify() {
    let endpoint =
        std::env::var("VEOVEO_SURREAL_ENDPOINT").expect("isolated store endpoint required");
    let config = StoreConfig::builder(
        endpoint,
        "veoveo_retention_test",
        format!("audit_{}", Uuid::now_v7().simple()),
        StoreCredentials::root("root", "root"),
    )
    .migrate_on_connect(true)
    .build()
    .unwrap();
    let store = PlatformStore::connect(config).await.unwrap();
    let now = Utc::now();
    let cutoff = now - TimeDelta::days(1);
    store
        .client()
        .query(
            "
        FOR $kind IN ['gateway_auth', 'gateway_policy', 'gateway_tool_call', 'recording'] {
            FOR $n IN 0..=$count {
                CREATE audit_event SET action = 'retention.fixture', resource_type = $kind,
                    outcome = 'allowed', occurred_at = $expired;
            };
            CREATE audit_event SET action = 'retention.fixture', resource_type = $kind,
                outcome = 'allowed', occurred_at = $cutoff;
        };
    ",
        )
        .bind(("count", GATEWAY_AUDIT_BATCH_LIMIT))
        .bind(("expired", cutoff - TimeDelta::seconds(1)))
        .bind(("cutoff", cutoff))
        .await
        .unwrap()
        .check()
        .unwrap();
    let mut response = store
        .client()
        .query(
            "
        SELECT VALUE id FROM audit_event WITH INDEX audit_event_resource_time
        WHERE resource_type = 'gateway_auth' AND occurred_at < $cutoff
        LIMIT 1024 EXPLAIN;
    ",
        )
        .bind(("cutoff", cutoff))
        .await
        .unwrap()
        .check()
        .unwrap();
    let plan: Value = response.take(0).unwrap();
    let plan = format!("{plan:?}");
    println!("retention selection plan: {plan}");
    assert!(
        plan.contains("audit_event_resource_time") && !plan.contains("TableScan"),
        "{plan}"
    );
    let state = GatewayState::new(store.clone());
    let batch = state.delete_audit_batch_before(cutoff).await.unwrap();
    assert!(batch.batch_was_full());
    assert_eq!(
        batch.auth_events_deleted,
        u64::from(GATEWAY_AUDIT_BATCH_LIMIT)
    );
    assert_eq!(
        batch.policy_events_deleted,
        u64::from(GATEWAY_AUDIT_BATCH_LIMIT)
    );
    assert_eq!(
        batch.tool_call_events_deleted,
        u64::from(GATEWAY_AUDIT_BATCH_LIMIT)
    );
    let tail = state.delete_audit_batch_before(cutoff).await.unwrap();
    assert!(!tail.batch_was_full());
    assert_eq!(
        (
            tail.auth_events_deleted,
            tail.policy_events_deleted,
            tail.tool_call_events_deleted
        ),
        (1, 1, 1)
    );
    let empty = state.delete_audit_batch_before(cutoff).await.unwrap();
    assert_eq!(
        (
            empty.auth_events_deleted,
            empty.policy_events_deleted,
            empty.tool_call_events_deleted
        ),
        (0, 0, 0)
    );
    let counts = state.audit_counts().await.unwrap();
    assert_eq!(
        (
            counts.auth_events,
            counts.policy_events,
            counts.tool_call_events
        ),
        (1, 1, 1)
    );
    let mut response = store
        .client()
        .query(
            "SELECT count() AS count FROM audit_event WHERE resource_type = 'recording' GROUP ALL;",
        )
        .await
        .unwrap()
        .check()
        .unwrap();
    #[derive(serde::Deserialize, SurrealValue)]
    struct Count {
        count: u64,
    }
    let counts: Vec<Count> = response.take(0).unwrap();
    assert_eq!(counts[0].count, u64::from(GATEWAY_AUDIT_BATCH_LIMIT) + 2);
}
