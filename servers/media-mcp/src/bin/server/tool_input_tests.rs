//! Protocol admission only; the provider endpoint is deliberately unavailable.
use super::*;
use std::time::Duration;
use veoveo_mcp_contract::hosting::testing::{self, TestGateway};
#[path = "../../../../../testing/fixtures/store.rs"]
mod fixture;

#[tokio::test]
async fn unknown_tool_arguments_complete_without_provider_dispatch() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = fixture::TestDb::with_modules(vec![
            veoveo_media_mcp::schema::module_setup(
                fixture::module_lanes::execution("media").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let args = config::Args::try_parse_from([
            "server",
            "--public-base-url",
            "https://media.test",
            "--api-key",
            "fixture",
            "--webhook-secret",
            "fixture",
            "--surreal-endpoint",
            "ws://127.0.0.1:9/rpc",
            "--surreal-namespace",
            "fixture",
            "--surreal-database",
            "fixture",
            "--surreal-auth-level",
            "database",
            "--surreal-username",
            "fixture",
            "--surreal-password",
            "fixture",
            "--internal-trust-jwks",
            r#"{"keys":[]}"#,
        ])
        .unwrap();
        let state = Arc::new(AppState {
            provider: veoveo_media_mcp::provider::ProviderClient::new("fixture")
                .with_base("http://127.0.0.1:9")
                .unwrap(),
            http: reqwest::Client::new(),
            public_endpoint: veoveo_mcp_contract::PublicDeployment::new("https://media.test")
                .unwrap()
                .server("media")
                .unwrap(),
            webhook_secret: secrecy::SecretString::from("fixture"),
            registry: tokio::sync::RwLock::new(None),
            tasks: TaskRuntime::new(db.a.clone(), "media", "strict-input"),
            durable: veoveo_media_mcp::state::MediaState::new(db.a.clone()),
            artifacts: ArtifactRepository::new("http://127.0.0.1:9"),
            retention: args.retention_policy(),
            subscribers: SubscriptionHub::new(),
        });
        let handler = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<MediaMcp>()
                .handler(move || Hosted::new(MediaMcp::new(handler.clone())))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments = json!({"model":"fixture/image","input":{}});
        let _: RunArgs = serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc("tools/call", json!({"name":"run","arguments":arguments}))
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
        assert!(state.registry.read().await.is_none());
    })
    .await
    .expect("Media argument admission exceeded 120 seconds");
}
