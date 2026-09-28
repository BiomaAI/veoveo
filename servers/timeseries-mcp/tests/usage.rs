#[path = "../../../testing/fixtures/store.rs"]
mod store;

use std::{collections::BTreeSet, time::Duration};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, OpenObject, task_record_id};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRuntime};
use veoveo_timeseries_mcp::{
    contract::{TimeseriesTaskUsageUri, TimeseriesUsageIndexUri},
    usage::TimeseriesUsage,
};
use veoveo_types::TaskId;

fn owner(principal: &str, labels: &[&str]) -> TaskOwner {
    serde_json::from_value(serde_json::json!({
        "principal_key":principal,"principal_kind":"service","issuer":"https://usage.test",
        "subject":principal,"profile":"operator","tenant_key":"tenant-a","data_labels":labels,
        "authority":{"work_context":"forecast","tenant":"tenant-a",
            "membership":"contributor","policy_revision":"test-1",
            "output_policy":{"owner":{"kind":"principal","id":principal}},
            "provenance":{"mode":"automated"}}
    }))
    .unwrap()
}

async fn create(tasks: &TaskRuntime, owner: &TaskOwner, number: u64) -> TaskId {
    let task_id = format!("0195dabe-7777-7abc-8def-{number:012x}")
        .parse()
        .unwrap();
    tasks
        .create(CreateTask {
            task_id,
            owner: owner.clone(),
            server: tasks.server().to_owned(),
            task_type: "forecast".into(),
            request: serde_json::json!({}),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id,
            server: tasks.server().to_owned(),
            source_id: None,
            provider_job_id: None,
            model_id: "timeseries/naive-trend".into(),
            kind: DomainUsageKind::Actual,
            quantity: Some(12.),
            unit: Some("source_row".into()),
            amount: None,
            currency: None,
            recorded_at: chrono::Utc::now(),
            metadata: OpenObject::default(),
        })
        .await
        .unwrap();
    task_id
}

#[tokio::test]
async fn usage_reader_pages_visible_forecasts_and_rechecks_authority_on_every_read() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "timeseries", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "timeseries", "reader");
        let foreign = TaskRuntime::new(db.a.clone(), "duckdb", "foreign");
        assert!(TimeseriesUsage::new(&foreign).is_err());
        let usage = TimeseriesUsage::new(&reader).unwrap();
        let caller = owner("owner", &["mission"]);
        let denied = owner("other", &["mission"]);
        // The old raw 100-row page was entirely consumed by these denied Tasks.
        for number in 1..=105 {
            let id = create(&writer, &denied, number).await;
            db.a.client().query("UPDATE ONLY $task SET request.input = NONE RETURN NONE;")
                .bind(("task", task_record_id(id))).await.unwrap().check().unwrap();
        }
        create(&foreign, &caller, 106).await;
        let mut expected = Vec::new();
        for number in 200..=300 {
            expected.push(create(&writer, &caller, number).await);
        }
        let first = usage.page(&caller, None).await.unwrap();
        assert_eq!(first.usage().iter().map(|entry| entry.task_id()).collect::<Vec<_>>(), expected[..100]);
        let position = TimeseriesUsageIndexUri::new(first.next_cursor());
        let position = TimeseriesUsageIndexUri::parse(position.as_str()).unwrap();
        let second = usage.page(&caller, position.cursor()).await.unwrap();
        assert_eq!(second.usage().len(), 1);
        assert_eq!(second.usage()[0].task_id(), expected[100]);
        assert!(second.next_cursor().is_none());
        let uri = second.usage()[0].usage_uri();
        let rows = usage.task(&caller, uri).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quantity, Some(12.));
        assert!(usage.task(&denied, uri).await.unwrap().is_empty());

        // Neither a saved cursor nor a previously returned URI retains authority.
        db.a.client().query("UPDATE ONLY $task SET request.owner.data_labels = ['mission', 'secret'] RETURN NONE;")
            .bind(("task", task_record_id(uri.task_id()))).await.unwrap().check().unwrap();
        assert!(usage.page(&caller, position.cursor()).await.unwrap().usage().is_empty());
        assert!(usage.task(&caller, uri).await.unwrap().is_empty());
        let cleared = owner("owner", &["mission", "secret"]);
        assert_eq!(usage.task(&cleared, uri).await.unwrap().len(), 1);

        // The usage row's parent/tenant must agree; a valid owner alone is insufficient.
        db.a.client().query("UPDATE domain_usage SET tenant = tenant:wrong WHERE task = $task RETURN NONE;")
            .bind(("task", task_record_id(uri.task_id()))).await.unwrap().check().unwrap();
        assert!(usage.task(&cleared, uri).await.unwrap().is_empty());
        assert!(usage.page(&cleared, position.cursor()).await.unwrap().usage().is_empty());
        assert!(usage.task(&caller, &TimeseriesTaskUsageUri::new(TaskId::new()).unwrap()).await.unwrap().is_empty());
    }).await.expect("Timeseries usage qualification exceeded 90 seconds");
}
