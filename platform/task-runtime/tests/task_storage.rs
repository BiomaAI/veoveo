//! Native admission of the current Task controls; domain input stays opaque.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
use std::{collections::BTreeSet, time::Duration};
use surrealdb::types::{SurrealValue, Value};
use veoveo_platform_store::{PlatformStore, task_record_id};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskError, TaskOwner, TaskRuntime};
use veoveo_types::TaskId;

fn draft(input: serde_json::Value) -> CreateTask {
    let owner: TaskOwner = serde_json::from_value(serde_json::json!({
        "principal_key":"pilot","principal_kind":"service","issuer":"https://storage.invalid",
        "subject":"pilot","profile":"operator","tenant_key":"test","data_labels":["clearance"],
        "authority":{"work_context":"mission","tenant":"test","membership":"owner","policy_revision":"native",
            "output_policy":{"owner":{"kind":"principal","id":"pilot"},"initial_grants":[{"subject":{"kind":"group","id":"crew"},"level":"read"}],"data_labels":["output"]},"provenance":{"mode":"automated"}}
    })).unwrap();
    CreateTask {
        task_id: TaskId::new(),
        owner,
        server: "storage-test".into(),
        task_type: "native".parse().unwrap(),
        request: input,
        recovery_class: RecoveryClass::Resume,
        idempotency_key: None,
        ttl_ms: None,
        poll_interval_ms: Some(u64::MAX),
        retention_pins: BTreeSet::new(),
    }
}
async fn row(store: &PlatformStore, id: TaskId) -> Value {
    store
        .client()
        .query(include_str!("queries/storage/read.surql"))
        .bind(("task", task_record_id(id)))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}
fn object_at<'a>(row: &'a mut Value, fields: &[&str]) -> &'a mut surrealdb::types::Object {
    let mut value = row;
    for field in fields {
        let Value::Object(object) = value else {
            panic!("expected controlled object")
        };
        value = object.get_mut(*field).expect("controlled field exists");
    }
    let Value::Object(object) = value else {
        panic!("expected controlled object")
    };
    object
}

#[tokio::test]
async fn derived_update_token_preserves_nanoseconds_and_overrides_forged_values() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "storage-test", "writer");
        let task = runtime
            .create(draft(serde_json::json!(null)))
            .await
            .unwrap()
            .snapshot;
        for text in [
            "2026-10-05T00:00:00.123456789Z",
            "2026-10-05T00:00:00.123456790Z",
        ] {
            let timestamp: chrono::DateTime<chrono::Utc> = text.parse().unwrap();
            let mut content = row(&db.a, task.task_id).await;
            let Value::Object(ref mut fields) = content else {
                unreachable!()
            };
            fields.insert("updated_at", timestamp.into_value());
            fields.insert("updated_at_exact", "forged".into_value());
            fields.insert("created_at", timestamp.into_value());
            fields.insert("created_at_exact", "forged".into_value());
            db.a.client()
                .query(include_str!("queries/storage/write.surql"))
                .bind(("task", task_record_id(task.task_id)))
                .bind(("content", content))
                .await
                .unwrap()
                .check()
                .unwrap();
            let record: veoveo_platform_store::TaskRecord =
                veoveo_platform_store::TaskRecord::from_value(row(&db.a, task.task_id).await)
                    .unwrap();
            assert_eq!(record.updated_at, timestamp);
            assert_eq!(record.updated_at_exact.timestamp(), timestamp);
            assert_eq!(record.created_at, timestamp);
            assert_eq!(record.created_at_exact.timestamp(), timestamp);
            assert_eq!(
                runtime.get(task.task_id).await.unwrap().unwrap().updated_at,
                timestamp
            );
        }
        let claimed = runtime
            .claim(task.task_id, Duration::from_secs(30))
            .await
            .unwrap();
        let record =
            veoveo_platform_store::TaskRecord::from_value(row(&db.a, task.task_id).await).unwrap();
        assert_eq!(
            record.updated_at_exact.timestamp(),
            claimed.snapshot.updated_at
        );
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn current_task_envelopes_preserve_opaque_inputs_u64_and_reconnect() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "storage-test", "writer");
        let reconnected = TaskRuntime::new(
            db.connect_at(db.a.config().endpoint().as_str()).await,
            "storage-test",
            "reader",
        );
        for input in [
            serde_json::json!(null),
            serde_json::json!(true),
            serde_json::json!(u64::MAX),
            serde_json::json!("opaque"),
            serde_json::json!([null,{"provider":{"unknown":[u64::MAX]}}]),
        ] {
            let expected = runtime.create(draft(input.clone())).await.unwrap().snapshot;
            let actual = reconnected.get(expected.task_id).await.unwrap().unwrap();
            assert_eq!(actual.owner, expected.owner);
            assert_eq!(actual.request, input);
            assert_eq!(actual.poll_interval_ms, Some(u64::MAX));
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn malformed_control_writes_reject_atomically_and_authority_drift_fails_decode() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "storage-test", "writer");
        let task = runtime
            .create(draft(serde_json::json!({"provider":{"extra":true}})))
            .await
            .unwrap()
            .snapshot;
        let before = row(&db.a, task.task_id).await;
        for case in 0..9 {
            let mut invalid = before.clone();
            match case {
                0 => {
                    object_at(&mut invalid, &["request"]).insert("undeclared", true.into_value());
                }
                1 => {
                    object_at(&mut invalid, &["request"]).remove("input");
                }
                2 => {
                    object_at(&mut invalid, &["owner_context"]).remove("data_labels");
                }
                3 => {
                    object_at(&mut invalid, &["owner_context"])
                        .insert("undeclared", true.into_value());
                }
                4 => {
                    object_at(&mut invalid, &["owner_context", "authority"])
                        .insert("undeclared", true.into_value());
                }
                5 => {
                    object_at(&mut invalid, &["owner_context", "authority", "provenance"])
                        .insert("initiator", "pilot".into_value());
                }
                6 => {
                    object_at(&mut invalid, &["request"])
                        .insert("poll_interval_ms", (-1_i64).into_value());
                }
                7 | 8 => {
                    let policy = object_at(
                        &mut invalid,
                        &["owner_context", "authority", "output_policy"],
                    );
                    let Value::Array(grants) = policy.get_mut("initial_grants").unwrap() else {
                        unreachable!()
                    };
                    let Value::Object(grant) = &mut grants[0] else {
                        unreachable!()
                    };
                    if case == 7 {
                        grant.insert("undeclared", true.into_value());
                    } else {
                        let Value::Object(subject) = grant.get_mut("subject").unwrap() else {
                            unreachable!()
                        };
                        subject.insert("undeclared", true.into_value());
                    }
                }
                _ => unreachable!(),
            }
            assert!(
                db.a.client()
                    .query(include_str!("queries/storage/write.surql"))
                    .bind(("task", task_record_id(task.task_id)))
                    .bind(("content", invalid))
                    .await
                    .unwrap()
                    .check()
                    .is_err(),
                "invalid control case {case} admitted"
            );
            assert_eq!(
                row(&db.a, task.task_id).await,
                before,
                "invalid control case {case} changed row"
            );
        }
        let mut drift = before.clone();
        object_at(&mut drift, &["owner_context", "authority"])
            .insert("tenant", "other".into_value());
        db.a.client()
            .query(include_str!("queries/storage/write.surql"))
            .bind(("task", task_record_id(task.task_id)))
            .bind(("content", drift))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            runtime.get(task.task_id).await,
            Err(TaskError::InvalidRecord(_))
        ));
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn scheduling_overflow_rejects_before_identity_or_task_effects() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "storage-test", "writer");
        let before: Value = db.a.client().query(include_str!("queries/storage/identity_inventory.surql")).await.unwrap().check().unwrap().take(0).unwrap();
        for ttl in [300_000_000_000_000, i64::MAX as u64, u64::MAX] {
            let mut invalid = draft(serde_json::json!(null));
            invalid.ttl_ms = Some(ttl);
            assert!(matches!(runtime.create(invalid).await, Err(TaskError::InvalidRecord(message)) if message == "Task TTL exceeds supported deadline range"));
        }
        let after: Value = db.a.client().query(include_str!("queries/storage/identity_inventory.surql")).await.unwrap().check().unwrap().take(0).unwrap();
        assert_eq!(after, before);
        for ttl in [None, Some(0), Some(1001)] {
            let mut valid = draft(serde_json::json!(null));
            valid.ttl_ms = ttl;
            let snapshot = runtime.create(valid).await.unwrap().snapshot;
            assert_eq!(snapshot.ttl_ms, ttl);
        }
    }).await.unwrap();
}

#[tokio::test]
async fn scaled_timing_decimals_decode_exactly_and_nested_native_input_fails_closed() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "storage-test", "writer");
        let task = runtime
            .create(draft(serde_json::json!({"fraction":1.5})))
            .await
            .unwrap()
            .snapshot;
        let original = row(&db.a, task.task_id).await;
        for (text, expected) in [("1.0", 1), ("18446744073709551615.0", u64::MAX)] {
            let mut content = original.clone();
            object_at(&mut content, &["request"]).insert(
                "poll_interval_ms",
                Value::Number(surrealdb::types::Number::Decimal(text.parse().unwrap())),
            );
            db.a.client()
                .query(include_str!("queries/storage/write.surql"))
                .bind(("task", task_record_id(task.task_id)))
                .bind(("content", content))
                .await
                .unwrap()
                .check()
                .unwrap();
            let decoded = runtime.get(task.task_id).await.unwrap().unwrap();
            assert_eq!(decoded.poll_interval_ms, Some(expected));
            assert_eq!(decoded.request, serde_json::json!({"fraction":1.5}));
        }
        for native in [
            Value::RecordId(surrealdb::types::RecordId::new("provider", "native")),
            chrono::Utc::now().into_value(),
        ] {
            let mut content = original.clone();
            let mut input = surrealdb::types::Object::new();
            input.insert("nested", Value::Array(vec![native].into()));
            object_at(&mut content, &["request"]).insert("input", Value::Object(input));
            db.a.client()
                .query(include_str!("queries/storage/write.surql"))
                .bind(("task", task_record_id(task.task_id)))
                .bind(("content", content))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(runtime.get(task.task_id).await.is_err());
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn trusted_lifecycle_selects_exact_identity_without_exposing_payloads() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "storage-test", "lifecycle");
        let task = runtime.create(draft(serde_json::json!({"private":"input"}))).await.unwrap().snapshot;
        runtime.claim(task.task_id, Duration::from_secs(30)).await.unwrap();
        let result = serde_json::json!({"content":[],"structuredContent":{"opaque":[null,1,1.5]},"isError":true});
        let finished = runtime.transition(task.task_id, veoveo_task_runtime::TaskTransition::Succeeded { result_uri: None, message: "complete".into(), result: result.clone() }).await.unwrap();
        let tenant = veoveo_platform_store::deterministic_tenant_id("test").unwrap().record_id();
        #[derive(Debug, SurrealValue)]
        struct Plan {
            operator: String,
            #[surreal(default)]
            children: Vec<Plan>,
        }
        impl Plan {
            fn contains(&self, predicate: impl Fn(&str) -> bool + Copy) -> bool {
                predicate(&self.operator) || self.children.iter().any(|child| child.contains(predicate))
            }
        }
        let mut explained = db.a.client()
            .query(include_str!("queries/storage/lifecycle_explain.surql"))
            .bind(("task", task_record_id(task.task_id)))
            .bind(("server", veoveo_platform_store::RecordId::new("mcp_server", "storage-test")))
            .bind(("tenant", tenant.clone()))
            .bind(("types", vec!["native".to_owned()]))
            .await.unwrap().check().unwrap();
        let plan = Plan::from_value(explained.take::<Value>(0).unwrap()).unwrap();
        assert!(!plan.contains(|operator| operator == "TableScan" || operator == "IndexScan"), "direct lifecycle record read scanned: {plan:?}");
        assert!(plan.contains(|operator| operator == "RecordIdScan"), "direct lifecycle record fetch missing: {plan:?}");
        for case in 0..7 {
            let mut response = db.a.client().query(include_str!("queries/storage/lifecycle.surql"))
                .bind(("task", task_record_id(if case == 1 { TaskId::new() } else { task.task_id })))
                .bind(("server", veoveo_platform_store::RecordId::new("mcp_server", if case == 2 { "other" } else { "storage-test" })))
                .bind(("tenant", if case == 3 { veoveo_platform_store::deterministic_tenant_id("other").unwrap().record_id() } else { tenant.clone() }))
                .bind(("types", vec![if case == 4 { "other" } else { "native" }.to_owned()]))
                .bind(("expected_result", if case == 6 { Value::None } else {
                    let record = veoveo_platform_store::TaskRequestRecord {
                        input: if case == 5 { serde_json::json!({"content":[],"structuredContent":{"opaque":[null,1,2]},"isError":true}) } else { result.clone() },
                        status_message: None, ttl_ms: None, poll_interval_ms: None,
                    }.into_value();
                    let Value::Object(mut fields) = record else { unreachable!() };
                    fields.remove("input").unwrap()
                }))
                .await.unwrap().check().unwrap();
            let value: Value = response.take(0).unwrap();
            if (1..=4).contains(&case) { assert_eq!(value, Value::None); continue; }
            let Value::Object(metadata) = value else { panic!("lifecycle metadata missing"); };
            assert_eq!(metadata.get("result_matches"), Some(&Value::Bool(case != 5 && case != 6)));
            assert_eq!(metadata.get("status"), Some(&finished.status.into_value()));
            for key in ["input","request","owner_context","result","provider_job","provider_event"] { assert!(!metadata.contains_key(key), "lifecycle leaked {key}"); }
        }
    }).await.unwrap();
}

#[tokio::test]
async fn trusted_input_equality_and_presence_reveal_no_task_payload() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "storage-test", "input-proof");
        let input = serde_json::json!({"opaque":[null, 1.5, {"nested":"value"}]});
        let task = runtime.create(draft(input.clone())).await.unwrap().snapshot;
        #[derive(Debug, SurrealValue)]
        struct Facts {
            exists: bool,
            matches: bool,
        }
        for (id, expected, exists, matches) in [
            (task.task_id, input.clone(), true, true),
            (
                task.task_id,
                serde_json::json!({"opaque":[null, 1.5, {"nested":"changed"}]}),
                true,
                false,
            ),
            (TaskId::new(), input.clone(), false, false),
        ] {
            let mut payload = veoveo_platform_store::TaskRequestRecord {
                input: expected,
                status_message: None,
                ttl_ms: None,
                poll_interval_ms: None,
            }
            .into_value();
            let Value::Object(ref mut object) = payload else {
                unreachable!()
            };
            let expected = object.remove("input").unwrap();
            let mut response =
                db.a.client()
                    .query(include_str!("queries/storage/input_exists.surql"))
                    .bind(("task", task_record_id(id)))
                    .bind(("expected_input", expected))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            let facts = Facts::from_value(response.take::<Value>(0).unwrap()).unwrap();
            assert_eq!((facts.exists, facts.matches), (exists, matches));
        }
        let scalar = runtime
            .create(draft(serde_json::json!(null)))
            .await
            .unwrap()
            .snapshot;
        let mut response =
            db.a.client()
                .query(include_str!("queries/storage/input_exists.surql"))
                .bind(("task", task_record_id(scalar.task_id)))
                .bind(("expected_input", surrealdb::types::Object::new()))
                .await
                .unwrap()
                .check()
                .unwrap();
        let facts = Facts::from_value(response.take::<Value>(0).unwrap()).unwrap();
        assert!(facts.exists && !facts.matches);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn retained_product_address_commits_with_opaque_result_and_survives_restart() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = fixture::TestDb::new().await;
        for result in [
            serde_json::json!({"product": true}),
            serde_json::Value::Null,
        ] {
            let runtime = TaskRuntime::new(db.a.clone(), "storage-test", "writer");
            let created = runtime
                .create(draft(serde_json::json!({})))
                .await
                .unwrap()
                .snapshot;
            runtime
                .claim(created.task_id, Duration::from_secs(60))
                .await
                .unwrap();
            let before = row(&db.a, created.task_id).await;
            let mut invalid = before.clone();
            let Value::Object(ref mut fields) = invalid else {
                unreachable!()
            };
            fields.insert("result_uri", "fixture://products/retained".into_value());
            assert!(
                db.a.client()
                    .query(include_str!("queries/storage/write.surql"))
                    .bind(("task", task_record_id(created.task_id)))
                    .bind(("content", invalid))
                    .await
                    .unwrap()
                    .check()
                    .is_err(),
                "running Task cannot gain a product address"
            );
            assert_eq!(row(&db.a, created.task_id).await, before);
            let uri = veoveo_types::ResourceUri::new("fixture://products/retained").unwrap();
            let settled = runtime
                .transition(
                    created.task_id,
                    veoveo_task_runtime::TaskTransition::Succeeded {
                        message: "retained product".into(),
                        result: result.clone(),
                        result_uri: Some(uri.clone()),
                    },
                )
                .await
                .unwrap();
            assert_eq!(settled.result, Some(result.clone()));
            assert_eq!(settled.result_uri, Some(uri.clone()));
            let restarted = TaskRuntime::new(db.b.clone(), "storage-test", "restarted");
            let replay = restarted.get(created.task_id).await.unwrap().unwrap();
            assert_eq!(replay.result, Some(result));
            assert_eq!(replay.result_uri, Some(uri));
            assert_eq!(
                serde_json::from_value::<veoveo_task_runtime::TaskSnapshot>(
                    serde_json::to_value(&replay).unwrap()
                )
                .unwrap(),
                replay
            );
        }
    })
    .await
    .expect("retained Task product qualification exceeded 180 seconds");
}
