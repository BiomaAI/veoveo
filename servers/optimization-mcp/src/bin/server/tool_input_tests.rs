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
  let (root, state) = {
 let root = tempfile::tempdir().unwrap();
 let state = Arc::new(AppState {
 tasks: TaskRuntime::new(db.a.clone(),"optimization","strict-input"),
 artifacts: ArtifactRepository::new("http://127.0.0.1:9"),
 executor: veoveo_optimization_mcp::executor::ExecutorClient::with_default_limit(root.path().join("unavailable-executor")),
 executor_health: veoveo_optimization_mcp::executor::ExecutorHealth {ready:false,cuopt_version:String::new(),cuda_runtime_version:String::new(),gpu_name:String::new(),gpu_uuid:String::new(),compute_capability:String::new()},
 executor_slot:Arc::new(tokio::sync::Semaphore::new(1)),
 problem_store:veoveo_optimization_mcp::problem_store::ProblemStore::open(root.path(),1024).unwrap(),
 subscriptions:Arc::new(veoveo_mcp_contract::SubscriptionHub::new()),
 resource_observers:Arc::new(veoveo_mcp_contract::ResourceListObservers::new()),
 max_artifact_bytes:1024,max_executor_frame_bytes:1024,
 });
 (root,state)
};
  let gateway_state = state.clone();
  let gateway = TestGateway::new(testing::for_domain::<OptimizationMcp>().handler(move || Hosted::new(OptimizationMcp::new(gateway_state.clone()))).build());
  let discover = gateway.rpc("server/discover",json!({})).await;
  assert!(discover.get("error").is_none(), "{discover}");
  let mut arguments = json!({"solution_uri":"optimization://solution/solution-01983da0-0000-7000-8000-000000000000"});
  let _: veoveo_optimization_mcp::contract::VerifySolutionRequest = serde_json::from_value(arguments.clone()).unwrap();
  arguments["undeclared"] = true.into();
  let body = gateway.rpc("tools/call",json!({"name":"verify_solution","arguments":arguments})).await;
  assert!(body.get("error").is_none(), "{body}");
  assert_eq!(body["result"]["resultType"], "complete", "{body}");
  let result: rmcp::model::CallToolResult = serde_json::from_value(body["result"].clone()).unwrap();
  assert_eq!(result.is_error,Some(true));
  assert!(serde_json::to_string(&result.content).unwrap().contains("undeclared"));
  assert!(state.tasks.list().await.unwrap().is_empty());
  assert!(std::fs::read_dir(root.path()).unwrap().next().is_none(), "tool must not create files");
 }).await.expect("hosted argument admission exceeded 120 seconds");
}
