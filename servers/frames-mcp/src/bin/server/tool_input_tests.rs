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
            veoveo_frames_mcp::schema::module_setup(
                fixture::module_lanes::execution("frames").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let state = Arc::new(AppState {
            tasks: TaskRuntime::new(db.a.clone(), "frames", "strict-input"),
            frames: FramesState::new(db.a.clone()),
            artifacts: ArtifactRepository::new("http://127.0.0.1:9"),
            max_artifact_bytes: 1024,
            subscriptions: veoveo_mcp_contract::SubscriptionHub::new(),
        });
        let gateway_state = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<FramesMcp>()
                .handler(move || Hosted::new(FramesMcp::new(gateway_state.clone())))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments = serde_json::to_value(ConvertFrameRequest {
            target: CoordinateSpace::EcefWgs84,
            points: vec![veoveo_frames_mcp::contract::CoordinatePoint::Wgs84(
                veoveo_frames_mcp::contract::Wgs84Position {
                    latitude_degrees: 37.0,
                    longitude_degrees: -122.0,
                    ellipsoid_height_m: 10.0,
                },
            )],
            allow_approximation: false,
        })
        .unwrap();
        let _: ConvertFrameRequest = serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc(
                "tools/call",
                json!({"name":"convert_frame","arguments":arguments}),
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
        assert!(state.tasks.list().await.unwrap().is_empty());
    })
    .await
    .expect("hosted argument admission exceeded 120 seconds");
}
