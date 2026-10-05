//! Authenticated hosted protocol admission with an inert simulation adapter.
//! This fixture establishes no simulation or GPU execution.
use super::{service::UavSimMcp, test_support};
use crate::{
    adapter::{Adapter, FakeAdapter},
    contract::SessionRequest,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};

#[tokio::test]
async fn unknown_tool_arguments_complete_before_simulation_access() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = crate::server::test_support::database(
            crate::server::test_support::fixture::StoreBackend::Memory,
        )
        .await;
        let simulation = super::service::fake_state().unwrap();
        let adapter = Arc::new(Mutex::new(FakeAdapter::new(simulation.clone())));
        let state = test_support::state(
            &db.a,
            Arc::new(Adapter::Fake(adapter.clone())),
            "strict-input",
        );
        let handler = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<UavSimMcp>()
                .handler(move || Hosted::new(UavSimMcp::new(handler.clone())))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments = json!({"session_id":"native-session"});
        let _: SessionRequest = serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc(
                "tools/call",
                json!({"name":"get_simulation_state","arguments":arguments}),
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
        assert_eq!(adapter.lock().await.state(), simulation);
    })
    .await
    .expect("UAV argument admission exceeded 120 seconds");
}
