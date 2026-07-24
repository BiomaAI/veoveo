use std::sync::Arc;

use axum::Router;
use rmcp::{
    ClientHandler, RoleClient, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, ClientCapabilities, ClientInfo, ContentBlock,
        Implementation, ServerCapabilities, ServerInfo,
    },
    service::{NotificationContext, Peer, RequestContext},
    transport::{
        StreamableHttpClientTransport,
        streamable_http_client::StreamableHttpClientTransportConfig,
        streamable_http_server::{StreamableHttpService, session::local::LocalSessionManager},
    },
};
use tokio::sync::{Mutex, mpsc};
use veoveo_mcp_contract::notify_resource_list_changed;

#[derive(Clone, Default)]
struct NotificationState {
    peer: Arc<Mutex<Option<Peer<RoleServer>>>>,
}

#[derive(Clone, Default)]
struct NotificationServer {
    state: NotificationState,
}

impl ServerHandler for NotificationServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_resources_list_changed()
                .build(),
        )
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if request.name != "change" {
            return Err(rmcp::ErrorData::invalid_params("unknown tool", None));
        }
        *self.state.peer.lock().await = Some(context.peer.clone());
        notify_resource_list_changed(&context.peer).await;
        Ok(CallToolResult::success(vec![ContentBlock::text("changed")]))
    }
}

#[derive(Clone)]
struct NotificationClient {
    notifications: mpsc::UnboundedSender<()>,
}

impl ClientHandler for NotificationClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("notification-conformance", "1"),
        )
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.notifications.send(());
    }
}

#[tokio::test]
async fn notifications_share_the_session_before_and_after_a_response() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let state = NotificationState::default();
    let service: StreamableHttpService<NotificationServer, LocalSessionManager> =
        StreamableHttpService::new(
            {
                let state = state.clone();
                move || {
                    Ok(NotificationServer {
                        state: state.clone(),
                    })
                }
            },
            LocalSessionManager::default().into(),
            veoveo_mcp_contract::canonical_streamable_http_server_config(),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().nest_service("/mcp", service)).await
    });

    let endpoint = format!("http://{address}/mcp");
    let raw_client = reqwest::Client::new();
    let initialize = raw_client
        .post(&endpoint)
        .header("accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "framing-conformance", "version": "1"}
            }
        }))
        .send()
        .await?;
    assert!(
        initialize
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"))
    );
    assert!(initialize.headers().contains_key("mcp-session-id"));

    let (notification_tx, mut notification_rx) = mpsc::unbounded_channel();
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(endpoint),
    );
    let client = NotificationClient {
        notifications: notification_tx,
    }
    .serve(transport)
    .await?;

    let result = client
        .call_tool(CallToolRequestParams::new("change"))
        .await?;
    assert_eq!(
        result
            .content
            .first()
            .and_then(|content| content.as_text())
            .map(|content| content.text.as_str()),
        Some("changed")
    );
    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.recv())
        .await?
        .expect("in-response notification");

    let peer = state
        .peer
        .lock()
        .await
        .clone()
        .expect("tool captured its session peer");
    notify_resource_list_changed(&peer).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.recv())
        .await?
        .expect("post-response notification");

    client.cancel().await?;
    server.abort();
    Ok(())
}
