//! Hosted argument admission with an exited process and unavailable worker socket.
//! This fixture cannot establish GPU inference readiness.
use super::*;
use serde_json::json;
use veoveo_mcp_contract::hosting::testing::{self, TestGateway};
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;

#[tokio::test]
async fn unknown_tool_arguments_complete_before_transcription_admission() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = fixture::TestDb::new().await;
        let worker = Arc::new(WorkerProcess::unavailable().await.unwrap());
        assert!(worker.ready().await.is_err());
        let service = Arc::new(SpeechService::new(
            TaskRuntime::new(db.a.clone(), "speech", "strict-input"),
            HttpArtifactPlane::new("http://127.0.0.1:9"),
            worker,
            1,
            1,
        ));
        let handler = service.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<mcp::SpeechMcp>()
                .handler(move || Hosted::new(mcp::SpeechMcp::new(handler.clone())))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments =
            json!({"artifact_uri":"artifact://01983da0-0000-7000-8000-000000000000"});
        let _: veoveo_speech_contract::TranscribeRequest =
            serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc(
                "tools/call",
                json!({"name":"transcribe","arguments":arguments}),
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
        assert!(service.tasks.list().await.unwrap().is_empty());
        service
            .audit
            .shutdown(Duration::from_secs(5))
            .await
            .unwrap();
    })
    .await
    .expect("Speech argument admission exceeded 120 seconds");
}
