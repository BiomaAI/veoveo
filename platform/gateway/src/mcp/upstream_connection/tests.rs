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
        CallToolRequestParams, CallToolResponse, CallToolResult, ClientConfig,
        ErrorData as McpError, ServerCapabilities, ServerConfig,
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
    BodyLimit,
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
                        Fault::BodyLimit => {
                            let (parts, body) = request.into_parts();
                            match axum::body::to_bytes(body, 2 * 1024 * 1024).await {
                                Ok(body) => next.run(axum::http::Request::from_parts(parts, axum::body::Body::from(body))).await,
                                Err(_) => (StatusCode::PAYLOAD_TOO_LARGE, "request body exceeds 2097152 bytes").into_response(),
                            }
                        }
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

    async fn connect(&self) -> Result<RunningService<RoleClient, ClientConfig>, RequestError> {
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

/// Exercise the production gateway handler, HTTP middleware and SDK client together.
/// The subject is injected at the same extension boundary as authenticated HTTP.
#[tokio::test]
async fn gateway_preserves_body_rejection_without_replaying_or_poisoning_other_requests() {
    use crate::mcp::http_response::preserve_upstream_http_rejection;
    use axum::{body::Body, http::Request, middleware};
    use tower::ServiceExt;
    use veoveo_mcp_contract::GatewayControlPlane;

    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_store::TestDb::new().await;
        let source = Fixture::start(Fault::BodyLimit).await;
        let state = crate::GatewayState::new(db.a.clone())
            .bind_token_extensions(Default::default())
            .unwrap()
            .bind_oauth_client_resolver(std::sync::Arc::new(
                crate::oauth_clients::CatalogOAuthClientResolver,
            ))
            .unwrap();
        let mut plane: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../../../configs/gateway.local.json"))
                .unwrap();
        let manifest = plane
            .servers
            .iter_mut()
            .find(|server| server.slug.as_str() == "media")
            .unwrap();
        manifest.upstream.url =
            veoveo_mcp_contract::UpstreamUrl::new(source.endpoint.clone()).unwrap();
        manifest.upstream.health_url = manifest.upstream.url.clone();
        manifest.upstream.security = veoveo_mcp_contract::UpstreamTransportSecurity::LoopbackHttp;
        let gateway = super::super::task_ownership_tests::gateway(state.clone(), plane);
        let service = StreamableHttpService::new(
            move || Ok(gateway.clone()),
            veoveo_mcp_contract::stateless_session_manager(),
            veoveo_mcp_contract::canonical_streamable_http_server_config(),
        );
        let router = Router::new()
            .nest_service("/mcp/workspace", service)
            .layer(middleware::from_fn(
                veoveo_mcp_contract::enforce_serialized_mcp_response,
            ))
            .layer(middleware::from_fn(preserve_upstream_http_rejection))
            .layer(middleware::from_fn(
                crate::request_observation::observe_request,
            ));
        let request = |id: u32, value: String| {
            let mut meta = rmcp::model::RequestMetaObject::default();
            meta.set_protocol_version(ProtocolVersion::V_2026_07_28);
            meta.set_client_capabilities(rmcp::model::ClientCapabilities::default());
            let mut params = CallToolRequestParams::new("media__run").with_arguments(
                serde_json::Map::from_iter([("input".to_owned(), value.into())]),
            );
            params.meta = Some(meta);
            let message = rmcp::model::ClientJsonRpcMessage::request(
                rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                    params,
                )),
                rmcp::model::RequestId::Number(id.into()),
            );
            let mut request = Request::builder()
                .method("POST")
                .uri("/mcp/workspace")
                .header("host", "localhost")
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2026-07-28")
                .header("mcp-method", "tools/call")
                .header("mcp-name", "media__run")
                .body(Body::from(serde_json::to_vec(&message).unwrap()))
                .unwrap();
            let mut subject = super::super::task_ownership_tests::subject();
            subject.access_token.oauth_client_id = "workspace".parse().unwrap();
            request.extensions_mut().insert(subject);
            request
        };
        let (rejected, accepted) = tokio::join!(
            router
                .clone()
                .oneshot(request(1, "x".repeat(2 * 1024 * 1024))),
            router.oneshot(request(2, "small".to_owned()))
        );
        let rejected = rejected.unwrap();
        let accepted = accepted.unwrap();
        let status = rejected.status();
        let rejected = axum::body::to_bytes(rejected.into_body(), 65536)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::PAYLOAD_TOO_LARGE,
            "{}",
            String::from_utf8_lossy(&rejected)
        );
        let rejected: serde_json::Value = serde_json::from_slice(&rejected).unwrap();
        assert_eq!(rejected["error"]["code"], -32603);
        assert!(
            rejected["error"]["message"]
                .as_str()
                .unwrap()
                .contains("2097152")
        );
        assert_eq!(accepted.status(), StatusCode::OK);
        let accepted = axum::body::to_bytes(accepted.into_body(), 65536)
            .await
            .unwrap();
        let accepted: serde_json::Value = serde_json::from_slice(&accepted).unwrap();
        assert!(accepted.get("result").is_some(), "{accepted}");
        assert_eq!(
            source.attempts.load(Ordering::SeqCst),
            4,
            "one discovery and one POST per call"
        );
        assert_eq!(
            source.mutations.load(Ordering::SeqCst),
            1,
            "only the admitted small call executes"
        );
        state
            .audit_writer()
            .await
            .shutdown(Duration::from_secs(5))
            .await
            .unwrap();
    })
    .await
    .expect("gateway HTTP rejection regression exceeded 90 seconds");
}
