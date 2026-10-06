//! Hosted argument admission only; no provider, renderer or solver executes.
#[path = "../../../../../testing/fixtures/tool_inputs.rs"]
mod input_fixture;
use super::*;
use crate::store_fixture as fixture;
use serde_json::json;
use std::time::Duration;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};

#[tokio::test]
async fn unknown_tool_arguments_return_completed_error_before_domain_effects() {
    tokio::time::timeout(Duration::from_secs(120), async {
  let _ = rustls::crypto::ring::default_provider().install_default();
  let db = fixture::TestDb::new().await;
  let (root, state) = {
 let root = tempfile::tempdir().unwrap();
 let state = Arc::new(AppState::new(
 TaskRuntime::new(db.a.clone(),"duckdb","strict_input"),
 veoveo_duckdb_mcp::artifacts::ArtifactRepository::new("http://127.0.0.1:9"),
 veoveo_duckdb_mcp::engine::EngineSettings { memory_limit:"64MB".into(), threads:1, spill_dir:root.path().join("spill"), trusted_extensions:vec![], spatial_axis_policy:Default::default() },
 app_state::ServerDirs { database_dir:root.path().join("databases"), exchange_dir:root.path().join("exchange") },
 app_state::Caps {max_inline_rows:10,max_inline_bytes:1024,default_timeout_ms:1000,max_timeout_ms:1000},
 veoveo_duckdb_runtime::HttpsSourcePolicy::deny_network(),1024));
 (root,state)
};
  let gateway_state = state.clone();
  let gateway = TestGateway::new(testing::for_domain::<DuckdbMcp>().handler(move || Hosted::new(DuckdbMcp::new(gateway_state.clone()))).build());
  let discover = gateway.rpc("server/discover",json!({})).await;
  assert!(discover.get("error").is_none(), "{discover}");
  let mut arguments = json!({"db":"strict_input","sql":"CREATE TABLE effects (value INTEGER)","create_if_missing":true});
  let _: veoveo_duckdb_mcp::contract::DuckDbExecuteRequest = serde_json::from_value(arguments.clone()).unwrap();
  arguments["undeclared"] = true.into();
  let body = gateway.rpc("tools/call",json!({"name":"execute","arguments":arguments})).await;
  assert!(body.get("error").is_none(), "{body}");
  assert_eq!(body["result"]["resultType"], "complete", "{body}");
  let result: rmcp::model::CallToolResult = serde_json::from_value(body["result"].clone()).unwrap();
  assert_eq!(result.is_error,Some(true));
  assert!(serde_json::to_string(&result.content).unwrap().contains("undeclared"));
        let cases = input_fixture::ToolInputCase::load(include_bytes!("../../../testdata/controlled-inputs.json"));
        assert_eq!(cases.len(), 26);
        for case in cases {
            match case.tool.as_str() {
                "ingest" => { let _: veoveo_duckdb_mcp::contract::DuckDbIngestRequest = case.decode(); },
                "query" => { let _: veoveo_duckdb_mcp::contract::DuckDbQueryRequest = case.decode(); },
                "export" => { let _: veoveo_duckdb_mcp::contract::DuckDbExportRequest = case.decode(); },
                _ => panic!("unexpected fixture tool"),
            }
            for (location, arguments) in case.unknown_fields().into_iter().chain(case.invalid_values()) {
                let body = gateway.rpc("tools/call", json!({"name":case.tool,"arguments":arguments})).await;
                assert!(body.get("error").is_none(), "{} {location}: {body}", case.branch);
                assert_eq!(body["result"]["resultType"], "complete", "{} {location}: {body}", case.branch);
                let result: rmcp::model::CallToolResult = serde_json::from_value(body["result"].clone()).unwrap();
                assert_eq!(result.is_error, Some(true), "{} {location}: {body}", case.branch);
                case.assert_error(&location, &serde_json::to_string(&result.content).unwrap());
            }
        }
  assert!(state.tasks.list().await.unwrap().is_empty());
  assert!(std::fs::read_dir(root.path()).unwrap().next().is_none(), "tool must not create files");
 }).await.expect("hosted argument admission exceeded 120 seconds");
}
