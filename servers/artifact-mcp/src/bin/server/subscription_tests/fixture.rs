use super::auth::Signing;
use crate::{
    handler::AppState,
    subscriptions::{ArtifactSubscriptions, start_dispatcher},
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_service::{ArtifactObjectStore, ArtifactService, SurrealArtifactRepository};
use veoveo_mcp_contract::PlaneCaller;
use veoveo_platform_store::PlatformStore;

pub type Artifacts = ArtifactService<SurrealArtifactRepository, ArtifactObjectStore>;
pub type Client = rmcp::service::RunningService<rmcp::RoleClient, rmcp::model::ClientConfig>;

struct HttpServer {
    url: String,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl HttpServer {
    async fn start(router: axum::Router, stop: CancellationToken) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .unwrap();
        });
        Self { url, stop, task }
    }
}
impl Drop for HttpServer {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}

pub struct Fixture {
    pub signing: Signing,
    pub artifacts: Artifacts,
    pub owner: PlaneCaller,
    pub subscriptions: ArtifactSubscriptions,
    _artifact_http: HttpServer,
    mcp_http: HttpServer,
}
impl Fixture {
    pub async fn new(store: PlatformStore) -> Self {
        let signing = Signing::new();
        let owner = signing.caller("author", "mission");
        let blobs = ArtifactObjectStore::new(Arc::new(object_store::memory::InMemory::new()));
        let artifacts =
            Artifacts::new(SurrealArtifactRepository::new(store.clone()), blobs.clone());
        let service = Artifacts::new(SurrealArtifactRepository::new(store.clone()), blobs);
        let plane_auth = veoveo_artifact_service::PlaneAuthenticator::new(
            veoveo_mcp_contract::GATEWAY_INTERNAL_TOKEN_ISSUER
                .parse()
                .unwrap(),
            vec!["artifact".parse().unwrap()],
            signing.trust.clone(),
        );
        let artifact_http = HttpServer::start(
            veoveo_artifact_service::http::router(veoveo_artifact_service::http::AppState::new(
                service, plane_auth,
            )),
            CancellationToken::new(),
        )
        .await;
        let subscriptions = ArtifactSubscriptions::new(store.clone());
        let stop = CancellationToken::new();
        start_dispatcher(
            store,
            subscriptions.clone(),
            stop.child_token(),
            veoveo_platform_store::ChangefeedConsumerId::new(format!(
                "artifact/fixture/{}",
                uuid::Uuid::now_v7()
            ))
            .unwrap(),
        )
        .await
        .unwrap();
        let state = Arc::new(AppState {
            plane: HttpArtifactPlane::new(&artifact_http.url),
            subscriptions: subscriptions.clone(),
            public_base_url: "https://artifact.fixture".into(),
        });
        let router = super::super::hosted_server(
            state,
            &veoveo_mcp_contract::PublicDeployment::new("http://127.0.0.1").unwrap(),
            true,
            Vec::new(),
            signing.trust.clone(),
        )
        .unwrap()
        .into_router();
        let mcp_http = HttpServer::start(router, stop).await;
        Self {
            signing,
            artifacts,
            owner,
            subscriptions,
            _artifact_http: artifact_http,
            mcp_http,
        }
    }
    pub async fn sdk(&self, caller: &PlaneCaller) -> Client {
        use rmcp::{
            ClientServiceExt,
            model::*,
            transport::{
                StreamableHttpClientTransport,
                streamable_http_client::StreamableHttpClientTransportConfig,
            },
        };
        let transport = StreamableHttpClientTransport::with_client(
            reqwest::Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap(),
            StreamableHttpClientTransportConfig::with_uri(format!(
                "{}/artifact/mcp",
                self.mcp_http.url
            ))
            .auth_header(caller.bearer_token.clone()),
        );
        tokio::time::timeout(
            Duration::from_secs(10),
            ClientConfig::new(
                ClientCapabilities::default(),
                Implementation::new("artifact-fixture", "1"),
            )
            .serve_with_lifecycle(
                transport,
                rmcp::ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            ),
        )
        .await
        .unwrap()
        .unwrap()
    }
}
