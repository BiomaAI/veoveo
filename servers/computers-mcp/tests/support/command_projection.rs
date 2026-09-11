//! Real SDK over the production HTTP router, with fixture-owned listeners.
use crate::signing::Signing;
use rmcp::{
    ClientServiceExt,
    model::*,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use veoveo_computers_mcp::Application;

pub struct Projection {
    endpoint: String,
    shutdown: CancellationToken,
    job: tokio::task::JoinHandle<()>,
}
impl Drop for Projection {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.job.abort();
    }
}
impl Projection {
    pub async fn new(app: Application, signing: &Signing) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let shutdown = CancellationToken::new();
        let router = veoveo_computers_mcp::server::router(
            Arc::new(app),
            signing.verifier.clone(),
            vec![address.to_string()],
            veoveo_computers_mcp::server::BrowserOrigins::new(vec![format!("http://{address}")])
                .unwrap(),
            shutdown.clone(),
        )
        .unwrap();
        let stop = shutdown.clone();
        let job = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            endpoint: format!("http://{address}/computers/mcp"),
            shutdown,
            job,
        }
    }
    pub async fn client(
        &self,
        bearer: String,
    ) -> rmcp::service::RunningService<rmcp::RoleClient, ClientInfo> {
        let transport = StreamableHttpClientTransport::with_client(
            reqwest::Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            StreamableHttpClientTransportConfig::with_uri(self.endpoint.as_str())
                .auth_header(bearer),
        );
        tokio::time::timeout(
            Duration::from_secs(10),
            ClientInfo::new(
                ClientCapabilities::builder().enable_tasks().build(),
                Implementation::new("native-computer-command", "1"),
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
pub fn call(name: &'static str, value: &impl serde::Serialize) -> CallToolRequestParams {
    CallToolRequestParams::new(name).with_arguments(
        serde_json::to_value(value)
            .unwrap()
            .as_object()
            .unwrap()
            .clone(),
    )
}

pub struct Schedulers {
    shutdown: CancellationToken,
    jobs: Vec<tokio::task::JoinHandle<()>>,
}
impl Schedulers {
    pub fn start(
        workers: impl IntoIterator<Item = Arc<veoveo_computers_mcp::CommandWorker>>,
    ) -> Self {
        let shutdown = CancellationToken::new();
        let jobs = workers
            .into_iter()
            .map(|worker| tokio::spawn(worker.run(shutdown.clone())))
            .collect();
        Self { shutdown, jobs }
    }
    pub async fn stop(mut self) {
        self.shutdown.cancel();
        for mut job in self.jobs.drain(..) {
            match tokio::time::timeout(Duration::from_secs(5), &mut job).await {
                Ok(result) => result.expect("native command scheduler panicked"),
                Err(_) => {
                    job.abort();
                    let _ = job.await;
                    panic!("native command scheduler did not stop");
                }
            }
        }
    }
}
impl Drop for Schedulers {
    fn drop(&mut self) {
        self.shutdown.cancel();
        for job in &self.jobs {
            job.abort();
        }
    }
}
