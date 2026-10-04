//! Hosted argument admission only; no provider, renderer or solver executes.
use super::*;
use serde_json::json;
use std::time::Duration;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};
#[path = "../../../../../testing/fixtures/store.rs"]
mod fixture;

#[tokio::test]
async fn unknown_tool_arguments_return_completed_error_before_domain_effects() {
    tokio::time::timeout(Duration::from_secs(120), async {
  let _ = rustls::crypto::ring::default_provider().install_default();
  let db = fixture::TestDb::new().await;
  let state = Arc::new(AppState {
 tasks: TaskRuntime::new(db.a.clone(),"timeseries","strict-input"),
 artifacts: ArtifactRepository::new("http://127.0.0.1:9"),
 source_policy: veoveo_duckdb_runtime::HttpsSourcePolicy::deny_network(),
 max_artifact_bytes:1024,
});
  let gateway_state = state.clone();
  let gateway = TestGateway::new(testing::for_domain::<TimeseriesMcp>().handler(move || Hosted::new(TimeseriesMcp::new(gateway_state.clone()))).build());
  let discover = gateway.rpc("server/discover",json!({})).await;
  assert!(discover.get("error").is_none(), "{discover}");
  let mut arguments = json!({"source":{"kind":"inline_csv","csv":"value\n1\n2\n"},"mapping":{"value_column":"value"},"horizon":4});
  let _: TimeseriesForecastRequest = serde_json::from_value(arguments.clone()).unwrap();
  arguments["undeclared"] = true.into();
  let body = gateway.rpc("tools/call",json!({"name":"forecast","arguments":arguments})).await;
  assert!(body.get("error").is_none(), "{body}");
  assert_eq!(body["result"]["resultType"], "complete", "{body}");
  let result: rmcp::model::CallToolResult = serde_json::from_value(body["result"].clone()).unwrap();
  assert_eq!(result.is_error,Some(true));
  assert!(serde_json::to_string(&result.content).unwrap().contains("undeclared"));
  assert!(state.tasks.list().await.unwrap().is_empty());

 }).await.expect("hosted argument admission exceeded 120 seconds");
}
