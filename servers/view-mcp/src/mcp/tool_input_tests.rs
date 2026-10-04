//! Authenticated protocol admission with a closed renderer channel.
//! No rendering, provider fetch or GPU acceptance occurs in this fixture.
use super::*;
use crate::store_fixture as fixture;
use serde_json::json;
use std::time::Duration;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};

#[tokio::test]
async fn unknown_tool_arguments_complete_before_view_changes() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = fixture::TestDb::new().await;
        let renderer = crate::renderer::RendererHandle::unavailable();
        assert!(!renderer.adapter().hardware_accelerated);
        let catalog = crate::source::LayerCatalog::from_definitions(
            vec![],
            crate::source::SourceConfig {
                raw_cache_bytes: 1024,
                max_response_bytes: 1024,
                request_timeout: Duration::from_secs(1),
            },
        )
        .unwrap();
        let views = Arc::new(crate::state::ViewService::new(
            crate::state::ViewServiceConfig {
                capture_limits: crate::contract::CaptureLimits {
                    max_width_px: 1,
                    max_height_px: 1,
                    max_pixels: 1,
                    max_deadline_ms: 1000,
                },
                max_views: 1,
                max_views_per_owner: 1,
                max_compositions: 1,
                max_compositions_per_owner: 1,
                max_frames: 1,
                max_frame_bytes: 1024,
                max_single_frame_bytes: 1024,
                decoded_cache_bytes: 1024,
                max_concurrent_loads: 1,
                max_tree_nodes: 1,
                detail_falloff_meters: 1.0,
            },
            catalog,
            renderer,
            veoveo_artifact_client::HttpArtifactPlane::new("http://127.0.0.1:9"),
        ));
        let state = Arc::new(AppState {
            views,
            tasks: veoveo_task_runtime::TaskRuntime::new(db.a.clone(), "view", "strict-input"),
            captures: tokio::sync::Semaphore::new(1),
            subscriptions: veoveo_mcp_contract::SubscriptionHub::new(),
        });
        let handler = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<ViewMcp>()
                .handler(move || Hosted::new(ViewMcp::new(handler.clone())))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments = json!({"view_id":"view-fixture","expected_revision":1});
        let _: CloseViewRequest = serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc(
                "tools/call",
                json!({"name":"close_view","arguments":arguments}),
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
    .expect("View argument admission exceeded 120 seconds");
}
