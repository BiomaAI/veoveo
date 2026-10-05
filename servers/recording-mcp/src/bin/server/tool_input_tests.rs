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
        let db = fixture::TestDb::with_modules(vec![
            veoveo_recording_store::schema::module_setup(
                fixture::module_lanes::execution("recordings").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let (root, state) = {
            let root = tempfile::tempdir().unwrap();
            let state = Arc::new(AppState {
                recordings: RecordingService::new(
                    db.a.clone(),
                    HttpArtifactPlane::new("http://127.0.0.1:9"),
                    root.path().to_path_buf(),
                )
                .unwrap(),
                playback: PlaybackManager::new(
                    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                    "http://127.0.0.1:9",
                    db.a.clone(),
                )
                .unwrap(),
                subscribers: SubscriptionHub::new(),
            });
            (root, state)
        };
        let gateway_state = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<mcp::RecordingMcp>()
                .handler(move || Hosted::new(mcp::RecordingMcp::new(gateway_state.clone())))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments = json!({"recording_id":"01983da0-0000-7000-8000-000000000000"});
        let _: veoveo_recording_mcp::contract::SealRecordingRequest =
            serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc(
                "tools/call",
                json!({"name":"seal_recording","arguments":arguments}),
            )
            .await;
        assert!(body.get("error").is_none(), "{body}");
        assert_eq!(body["result"]["resultType"], "complete", "{body}");
        let result: rmcp::model::CallToolResult =
            serde_json::from_value(body["result"].clone()).unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(
            serde_json::to_string(&result.content)
                .unwrap()
                .contains("undeclared")
        );
        let tasks =
            veoveo_task_runtime::TaskRuntime::new(db.a.clone(), "recording", "strict-input");
        assert!(tasks.list().await.unwrap().is_empty());
        assert!(
            std::fs::read_dir(root.path()).unwrap().next().is_none(),
            "tool must not create files"
        );
    })
    .await
    .expect("hosted argument admission exceeded 120 seconds");
}
