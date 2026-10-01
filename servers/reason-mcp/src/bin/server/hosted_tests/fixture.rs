use super::auth::Signing;
use crate::{AppState, host};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use veoveo_artifact_service::{ArtifactObjectStore, ArtifactService, SurrealArtifactRepository};
use veoveo_mcp_contract::PlaneCaller;
use veoveo_platform_store::PlatformStore;
use veoveo_reason_mcp::{
    artifacts::ArtifactRepository,
    catalog::{ModelConfig, PipelineCatalog, PipelineConfig},
    executor::ReasonExecutor,
};
use veoveo_recording_reader::{
    RecordingReader,
    cache::{LayerCache, LayerCacheLimits},
};
use veoveo_recording_video::runtime::VideoSourceLimits;
use veoveo_task_runtime::TaskRuntime;

pub type Artifacts = ArtifactService<SurrealArtifactRepository, ArtifactObjectStore>;

pub struct HttpServer {
    pub address: SocketAddr,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl HttpServer {
    async fn serve(
        listener: tokio::net::TcpListener,
        router: axum::Router,
        cancel: CancellationToken,
    ) -> Self {
        let address = listener.local_addr().unwrap();
        let shutdown = cancel.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            address,
            cancel,
            task,
        }
    }
    pub async fn stop(mut self) {
        self.cancel.cancel();
        // Graceful shutdown closes keep-alive connections and drops the old
        // router state before the replacement binds the same endpoint.
        tokio::time::timeout(Duration::from_secs(5), &mut self.task)
            .await
            .expect("HTTP fixture did not stop")
            .expect("HTTP fixture failed");
    }
}
impl Drop for HttpServer {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.task.abort();
    }
}

pub struct Fixture {
    pub store: PlatformStore,
    pub artifacts: Artifacts,
    pub owner: PlaneCaller,
    pub signing: Signing,
    pub content_reads: Arc<std::sync::atomic::AtomicUsize>,
    artifact_http: HttpServer,
    temp: tempfile::TempDir,
}
impl Fixture {
    pub async fn new(store: PlatformStore) -> Arc<Self> {
        let signing = Signing::new();
        let owner = signing.caller("author", "mission");
        let blobs = ArtifactObjectStore::new(Arc::new(object_store::memory::InMemory::new()));
        let artifacts =
            Artifacts::new(SurrealArtifactRepository::new(store.clone()), blobs.clone());
        let service = Artifacts::new(SurrealArtifactRepository::new(store.clone()), blobs);
        let auth = veoveo_artifact_service::PlaneAuthenticator::new(
            veoveo_mcp_contract::GATEWAY_INTERNAL_TOKEN_ISSUER
                .parse()
                .unwrap(),
            vec!["reason".parse().unwrap()],
            signing.trust.clone(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let content_reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count_reads = content_reads.clone();
        let artifact_router = veoveo_artifact_service::http::router(
            veoveo_artifact_service::http::AppState::new(service, auth),
        )
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let counter = count_reads.clone();
                async move {
                    let path = request.uri().path();
                    if request.method() == axum::http::Method::GET
                        && path.starts_with("/artifacts/")
                        && (path.split('/').count() == 3 || path.ends_with("/download"))
                    {
                        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    next.run(request).await
                }
            },
        ));
        let artifact_http =
            HttpServer::serve(listener, artifact_router, CancellationToken::new()).await;
        Arc::new(Self {
            store,
            artifacts,
            owner,
            signing,
            content_reads,
            artifact_http,
            temp: tempfile::tempdir().unwrap(),
        })
    }
    fn state(&self) -> Arc<AppState> {
        let artifact_url = format!("http://{}", self.artifact_http.address);
        let spool = self.temp.path().join("spool");
        std::fs::create_dir_all(&spool).unwrap();
        let recordings = RecordingReader::new(
            self.store.clone(),
            spool,
            LayerCache::new(
                self.temp.path().join("cache"),
                LayerCacheLimits {
                    managed_bytes: 1024 * 1024,
                    minimum_free_bytes: 1024,
                },
                veoveo_artifact_client::HttpArtifactPlane::new(&artifact_url),
            )
            .unwrap(),
        )
        .unwrap();
        #[derive(serde::Deserialize)]
        struct Catalog {
            models: Vec<ModelConfig>,
            pipelines: Vec<PipelineConfig>,
        }
        let catalog: Catalog = serde_json::from_str(include_str!(
            "../../../../../../configs/reason/catalog.example.json"
        ))
        .unwrap();
        Arc::new(AppState {
            tasks: TaskRuntime::new(
                self.store.clone(),
                "reason",
                uuid::Uuid::now_v7().to_string(),
            ),
            finding_changes: veoveo_reason_mcp::knowledge::observe::FindingChanges::new(
                self.store.clone(),
            ),
            artifacts: ArtifactRepository::new(artifact_url),
            recordings: Arc::new(recordings),
            catalog: Arc::new(PipelineCatalog::new(catalog.models, catalog.pipelines).unwrap()),
            // This nonexistent path makes accidental inference fail. The test does
            // not select GPU readiness or invoke the analysis tool.
            executor: ReasonExecutor::new(
                self.temp.path().join("inference-is-not-part-of-this-test"),
                Duration::from_secs(1),
                10,
                4096,
                8192,
            )
            .unwrap(),
            source_limits: VideoSourceLimits {
                max_samples: 10,
                max_encoded_bytes: 1024,
                max_segment_bytes: 1024,
            },
            max_artifact_bytes: 1024 * 1024,
            max_inline_resource_bytes: 1024 * 1024,
            max_grounding_bytes: 1024,
            work_slots: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }
    pub async fn reason(&self, address: Option<SocketAddr>) -> HttpServer {
        let listener = tokio::net::TcpListener::bind(
            address.unwrap_or_else(|| "127.0.0.1:0".parse().unwrap()),
        )
        .await
        .unwrap();
        let address = listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let router = host::router(
            self.state(),
            self.signing.verifier(),
            Arc::new(vec![address.to_string()]),
            "/reason",
            cancel.clone(),
        );
        HttpServer::serve(listener, router, cancel).await
    }
    pub async fn sdk(
        &self,
        address: SocketAddr,
        caller: &PlaneCaller,
    ) -> rmcp::service::RunningService<rmcp::RoleClient, rmcp::model::ClientConfig> {
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
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap(),
            StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/reason/mcp"))
                .auth_header(caller.bearer_token.clone()),
        );
        tokio::time::timeout(
            Duration::from_secs(10),
            ClientConfig::new(
                ClientCapabilities::default(),
                Implementation::new("reason-fixture", "1"),
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
