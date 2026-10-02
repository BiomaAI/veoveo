use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Router, http::StatusCode, response::IntoResponse};
use base64::Engine as _;
use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ClientConfig, ServerCapabilities,
        ServerConfig,
    },
    service::{RequestContext, RoleServer},
    transport::streamable_http_server::StreamableHttpService,
};
use tokio::{io::AsyncReadExt, net::TcpListener};
use veoveo_mcp_contract::{GatewayInternalSigningKey, GatewayInternalTokenIssuer, TokenIssuer};

#[derive(Clone)]
struct Source(Arc<AtomicUsize>);
impl ServerHandler for Source {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn call_tool(
        &self,
        _: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(CallToolResult::success(vec![]).into())
    }
}

#[derive(Clone, Copy)]
enum Fault {
    Disconnect(usize),
    Unauthorized,
    ProtocolError,
    InvalidJson,
}

struct Fixture {
    endpoint: String,
    attempts: Arc<AtomicUsize>,
    mutations: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn start(fault: Fault) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let attempts = Arc::new(AtomicUsize::new(0));
        let mutations = Arc::new(AtomicUsize::new(0));
        let source = Source(mutations.clone());
        let service = StreamableHttpService::new(
            move || Ok(source.clone()),
            veoveo_mcp_contract::stateless_session_manager(),
            veoveo_mcp_contract::canonical_streamable_http_server_config(),
        );
        let count = attempts.clone();
        let router = Router::new().nest_service("/mcp", service).layer(
            axum::middleware::from_fn(move |request: axum::extract::Request, next: axum::middleware::Next| {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    match fault {
                        Fault::Disconnect(_) => next.run(request).await,
                        Fault::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
                        Fault::InvalidJson => ([("content-type", "application/json")], "invalid").into_response(),
                        Fault::ProtocolError => {
                            let bytes = axum::body::to_bytes(request.into_body(), 65536).await.unwrap();
                            let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                            axum::Json(serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "error":{"code":-32603,"message":"source failure"}})).into_response()
                        }
                    }
                }
            }),
        );
        let disconnected = attempts.clone();
        let server = tokio::spawn(async move {
            if let Fault::Disconnect(times) = fault {
                for _ in 0..times {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut bytes = [0; 4096];
                    assert!(socket.read(&mut bytes).await.unwrap() > 0);
                    disconnected.fetch_add(1, Ordering::SeqCst);
                    // The discovery POST reached the old connection, which dies
                    // without an HTTP response during upstream replacement.
                    drop(socket);
                }
            }
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            endpoint,
            attempts,
            mutations,
            server,
        }
    }

    async fn connect(&self) -> Result<RunningService<RoleClient, ClientConfig>, McpError> {
        let issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::new("test-gateway").unwrap(),
            GatewayInternalSigningKey::new(
                "test",
                base64::engine::general_purpose::STANDARD
                    .decode("MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl")
                    .unwrap(),
            )
            .unwrap(),
        );
        let http = GatewayAuthorizedHttpClient::new(
            reqwest::Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            issuer,
            "workspace".parse().unwrap(),
            "media".parse().unwrap(),
            &super::super::task_ownership_tests::subject(),
            None,
        );
        discover(
            ClientConfig::default(),
            http,
            StreamableHttpClientTransportConfig::with_uri(self.endpoint.clone()),
        )
        .await
    }
}

#[tokio::test]
async fn discovery_recovers_one_disconnect_without_repeating_domain_mutations() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(30), async {
        let fixture = Fixture::start(Fault::Disconnect(1)).await;
        let client = fixture
            .connect()
            .await
            .expect("discovery should reconnect once");
        assert_eq!(fixture.attempts.load(Ordering::SeqCst), 2);
        client
            .call_tool(CallToolRequestParams::new("mutate"))
            .await
            .unwrap();
        assert_eq!(fixture.mutations.load(Ordering::SeqCst), 1);
        client.cancel().await.unwrap();

        for (fault, attempts) in [
            (Fault::Disconnect(2), 2),
            (Fault::Unauthorized, 1),
            (Fault::ProtocolError, 1),
            (Fault::InvalidJson, 1),
        ] {
            let fixture = Fixture::start(fault).await;
            assert!(fixture.connect().await.is_err());
            assert_eq!(fixture.attempts.load(Ordering::SeqCst), attempts);
            assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
        }
    })
    .await
    .expect("discovery recovery matrix exceeded thirty seconds");
}
