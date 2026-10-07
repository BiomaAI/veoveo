//! Hosted argument admission only; no provider, renderer or solver executes.
#[path = "../../../../../testing/fixtures/tool_inputs.rs"]
mod input_fixture;
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
  let mut arguments = json!({"source":{"kind":"inline_csv","csv":"value\n1\n2\n"},"mapping":{"valueColumn":"value"},"horizon":4});
  let _: TimeseriesForecastRequest = serde_json::from_value(arguments.clone()).unwrap();
  arguments["undeclared"] = true.into();
  let body = gateway.rpc("tools/call",json!({"name":"forecast","arguments":arguments})).await;
  assert!(body.get("error").is_none(), "{body}");
  assert_eq!(body["result"]["resultType"], "complete", "{body}");
  let result: rmcp::model::CallToolResult = serde_json::from_value(body["result"].clone()).unwrap();
  assert_eq!(result.is_error,Some(true));
  assert!(serde_json::to_string(&result.content).unwrap().contains("undeclared"));
        for mixed in [false, true] {
            let mut arguments = json!({"source":{"kind":"inline_csv","csv":"value\n1\n2\n"},"mapping":{"valueColumn":"value"},"horizon":4});
            arguments["mapping"]["value_column"] = arguments["mapping"]["valueColumn"].clone();
            if !mixed { arguments["mapping"].as_object_mut().unwrap().remove("valueColumn"); }
            let body = gateway.rpc("tools/call", json!({"name":"forecast","arguments":arguments})).await;
            assert!(body.get("error").is_none(), "{body}");
            assert_eq!(body["result"]["resultType"], "complete", "{body}");
            let result: rmcp::model::CallToolResult = serde_json::from_value(body["result"].clone()).unwrap();
            assert_eq!(result.is_error, Some(true));
            assert!(serde_json::to_string(&result.content).unwrap().contains("value_column"));
        }
        let cases = input_fixture::ToolInputCase::load(include_bytes!("../../../testdata/controlled-inputs.json"));
        assert_eq!(cases.len(), 24);
        for case in cases {
            match case.tool.as_str() {
                "forecast" => { let _: veoveo_timeseries_mcp::contract::TimeseriesForecastRequest = case.decode(); },
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

 }).await.expect("hosted argument admission exceeded 120 seconds");
}
