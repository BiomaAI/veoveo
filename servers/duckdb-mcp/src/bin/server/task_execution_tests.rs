//! Native engine and Store checks; no Artifact service or GPU workload is used.
use super::*;
use crate::app_state::{Caps, ServerDirs};
use serde_json::json;
use veoveo_duckdb_mcp::{
    artifacts::ArtifactRepository, engine::EngineSettings, usage::DuckDbUsage,
};
use veoveo_mcp_contract::{ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret};
use veoveo_task_runtime::{TaskPayloadState, TaskRuntime};

#[path = "../../../../../testing/fixtures/store.rs"]
mod store;

#[tokio::test]
async fn execution_and_recovered_query_keep_native_ids_in_results_and_usage() {
    tokio::time::timeout(Duration::from_secs(90), async {
        crate::install_rustls_provider();
        let db = store::TestDb::new().await;
        let observer = TaskRuntime::new(db.b.clone(), "duckdb", "execution-observer");
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(
            TaskRuntime::new(db.a.clone(), "duckdb", "execution-test"),
            ArtifactRepository::new("http://127.0.0.1:0"),
            EngineSettings::new(directory.path().join("spill")),
            ServerDirs { database_dir: directory.path().join("databases"), exchange_dir: directory.path().join("exchange") },
            Caps { max_inline_rows: 100, max_inline_bytes: 1024 * 1024, default_timeout_ms: 10_000, max_timeout_ms: 10_000 },
            veoveo_duckdb_runtime::HttpsSourcePolicy::new(vec![]),
            1024 * 1024,
        ));
        let identity = crate::test_support::identity("operator", "native-test");
        let owner = runtime_owner(&identity);
        let operations = [
            ("execute", json!({"db":"metrics","sql":"CREATE TABLE facts AS SELECT 42 AS answer","create_if_missing":true}), false),
            ("query", json!({"db":"metrics","sql":"SELECT answer FROM facts"}), false),
            ("query", json!({"db":"metrics","sql":"SELECT * FROM must_not_execute"}), true),
        ];
        for (name, arguments, wrong_capability) in operations {
            let task_id = TaskId::new();
            let args = parse_task_args(name, arguments).unwrap();
            let capability = task_needs_artifact_capability(&args).then(|| IssuedArtifactWriteCapability {
                capability_id: ArtifactWriteCapabilityId::new(),
                secret: ArtifactWriteCapabilitySecret::new("n".repeat(32)).unwrap(),
                task_id: if wrong_capability { TaskId::new().to_string() } else { task_id.to_string() },
                expires_at: Utc::now() + TimeDelta::hours(1),
            });
            let recovery_class = task_recovery_class(&args);
            let task_type = args.task_type();
            let request = DuckdbTaskRequest { args, artifact_write_capability: capability };
            let created = state.tasks.create(DurableCreateTask {
                task_id, owner: owner.clone(), server: SERVER_SLUG.into(), task_type,
                request: serde_json::to_value(request).unwrap(), recovery_class,
                idempotency_key: None, ttl_ms: Some(MCP_TASK_TTL_MS), poll_interval_ms: None,
                retention_pins: BTreeSet::new(),
            }).await.unwrap();
            state.tasks.claim(task_id, TASK_LEASE_DURATION).await.unwrap();
            // Recover the exact retained identity and request before running the worker.
            let identity = identity_from_runtime(&created.snapshot.owner).unwrap();
            let request = serde_json::from_value(created.snapshot.request).unwrap();
            run_task(state.clone(), task_id, identity, request, None, CancellationToken::new()).await;
            let TaskPayloadState::Completed(payload) = state.tasks.payload_state(task_id).await.unwrap() else { panic!("Task did not settle"); };
            let usage = DuckDbUsage::new(&observer).unwrap().task(&owner, &veoveo_duckdb_mcp::contract::DuckDbTaskUsageUri::new(task_id).unwrap()).await.unwrap();
            if wrong_capability {
                assert_eq!(payload["isError"], true);
                assert!(payload["content"][0]["text"].as_str().unwrap().contains("capability belongs to another Task"));
                assert!(usage.is_empty());
                continue;
            }
            assert_ne!(payload["isError"], true, "{payload}");
            assert_eq!(usage.len(), 1);
            assert_eq!(usage[0].task, veoveo_platform_store::task_record_id(task_id));
            let details: DuckDbUsageDetails = serde_json::from_value(serde_json::to_value(&usage[0].metadata).unwrap()).unwrap();
            assert_eq!(details.operation().name().as_str(), name);
            if name == "query" {
                assert_eq!(payload["structuredContent"]["rows"], json!([[42]]));
                assert_eq!(details, DuckDbUsageDetails::Query { result: DuckDbQueryUsage::Inline { rows_returned: 1, truncated: false } });
            }
        }

        // Exercise the checked source options and preserve the quoted identifier
        // through real ingest, result metadata and a subsequent read.
        let table = "  Order \"Lines\"  ";
        let caller = veoveo_mcp_contract::PlaneCaller::from_gateway(
            identity.clone(),
            veoveo_mcp_contract::hosting::ForwardedBearer::new("fixture-bearer"),
        );
        let request = serde_json::from_value(json!({
            "db":"metrics", "table":table, "mode":"create",
            "source":{"kind":"inline_csv", "csv":"value\n43\nNA\n", "options":{
                "header":true, "extra":{"nullstr":["NA"],"sample_size":-1}
            }}
        })).unwrap();
        let ingested = crate::sql_ops::ingest_op(&state, &caller, &identity, request).await.unwrap();
        assert_eq!(ingested.table.as_str(), table);
        assert_eq!(ingested.rows_ingested, 2);
        let sql = format!("SELECT value FROM {} ORDER BY value NULLS LAST", veoveo_duckdb_mcp::contract::duckdb_quote_identifier(table));
        let query = DuckDbQueryRequest::builder("metrics".parse().unwrap(), sql.try_into().unwrap()).build().unwrap();
        let output = crate::sql_ops::query_op(&state, &crate::artifact_output::ArtifactWriter::caller(caller), &identity, query).await.unwrap();
        assert_eq!(output.rows(), &[vec![json!(43)], vec![json!(null)]]);
    }).await.expect("DuckDB execution qualification exceeded 90 seconds");
}
