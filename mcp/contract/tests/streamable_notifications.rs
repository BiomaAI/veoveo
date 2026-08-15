use axum::Router;
use rmcp::{
    ServerHandler,
    model::{ServerCapabilities, ServerInfo},
    transport::streamable_http_server::StreamableHttpService,
};

#[derive(Clone, Default)]
struct StatelessServer;

impl ServerHandler for StatelessServer {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        std::borrow::Cow::Owned(vec![rmcp::model::ProtocolVersion::V_2026_07_28])
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }
}

#[tokio::test]
async fn discover_is_an_ordinary_stateless_json_exchange() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let service = StreamableHttpService::new(
        || Ok(StatelessServer),
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().nest_service("/mcp", service)).await
    });

    let response = reqwest::Client::new()
        .post(format!("http://{address}/mcp"))
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2026-07-28")
        .header("mcp-method", "server/discover")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientInfo": {
                        "name": "stateless-conformance",
                        "version": "1"
                    },
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }))
        .send()
        .await?;

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(!response.headers().contains_key("mcp-session-id"));
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.split(';').next().unwrap_or(value)),
        Some("application/json")
    );
    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["result"]["resultType"], "complete");
    assert_eq!(body["result"]["supportedVersions"][0], "2026-07-28");

    server.abort();
    Ok(())
}
