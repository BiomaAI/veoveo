//! Synthetic fixed-route edges around the production relay; owned TLS trust lets
//! the unmodified client exercise its actual HTTPS WebSocket transport locally.
use axum::{
    Router,
    extract::{OriginalUri, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_computers_transport::{CliRelayMode, CliUpstreamRequest, Client, relay_cli};

pub struct Edges {
    pub endpoint: String,
    pub scoped: Arc<AtomicUsize>,
    pub root: Arc<AtomicUsize>,
    pub active: Arc<AtomicUsize>,
    stop: CancellationToken,
    jobs: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Edges {
    fn drop(&mut self) {
        self.stop.cancel();
        for job in &self.jobs {
            job.abort();
        }
    }
}
async fn serve(router: Router, stop: CancellationToken) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let job = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    (format!("http://{address}"), job)
}
impl Edges {
    pub async fn start(workers: [String; 2], computer: Uuid, directory: &Path) -> Self {
        let stop = CancellationToken::new();
        let mut jobs = Vec::new();
        let scoped = Arc::new(AtomicUsize::new(0));
        let root = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let mut destinations = workers
            .map(|url| url.trim_end_matches("/admin").to_owned())
            .to_vec();
        for mode in [CliRelayMode::Internal, CliRelayMode::PublicClient] {
            let client = Client::new(reqwest::Client::builder().no_proxy()).unwrap();
            let next = Arc::new(AtomicUsize::new(0));
            let token = stop.clone();
            let scoped_count = scoped.clone();
            let root_count = root.clone();
            let active_count = active.clone();
            let handler = move |OriginalUri(uri): OriginalUri,
                                headers: HeaderMap,
                                ws: WebSocketUpgrade| {
                let client = client.clone();
                let destination =
                    destinations[next.fetch_add(1, Ordering::SeqCst) % destinations.len()].clone();
                let stop = token.clone();
                let scoped = scoped_count.clone();
                let root = root_count.clone();
                let active = active_count.clone();
                async move {
                    if headers.contains_key("origin") || uri.query().is_some() {
                        return StatusCode::FORBIDDEN.into_response();
                    }
                    let authorization = if mode == CliRelayMode::PublicClient {
                        let Some(token) =
                            headers.get("cf-access-token").and_then(|v| v.to_str().ok())
                        else {
                            return StatusCode::UNAUTHORIZED.into_response();
                        };
                        if token.len() != 107 {
                            return StatusCode::FORBIDDEN.into_response();
                        }
                        format!("Bearer {token}").parse().unwrap()
                    } else {
                        let Some(value) = headers.get("authorization") else {
                            return StatusCode::UNAUTHORIZED.into_response();
                        };
                        value.clone()
                    };
                    let is_root = uri.path() == "/_ws_tunnel";
                    if mode == CliRelayMode::PublicClient {
                        (if is_root { &root } else { &scoped }).fetch_add(1, Ordering::SeqCst);
                    }
                    let suffix = if is_root {
                        "/_ws_tunnel".to_owned()
                    } else {
                        format!("/computers/{computer}/_ws_tunnel")
                    };
                    let path = if mode == CliRelayMode::Internal {
                        if is_root {
                            "/cli/_ws_tunnel".to_owned()
                        } else {
                            format!("/cli/{computer}/_ws_tunnel")
                        }
                    } else {
                        suffix
                    };
                    let upstream = client
                        .connect_cli(CliUpstreamRequest {
                            url: format!("{destination}{path}").parse().unwrap(),
                            authorization,
                            host: None,
                        })
                        .await;
                    let Ok(upstream) = upstream else {
                        return StatusCode::BAD_GATEWAY.into_response();
                    };
                    ws.max_frame_size(65536)
                        .max_message_size(65536)
                        .write_buffer_size(0)
                        .max_write_buffer_size(131072)
                        .on_upgrade(move |socket| async move {
                            active.fetch_add(1, Ordering::SeqCst);
                            let _ = relay_cli(socket, upstream, mode, stop).await;
                            active.fetch_sub(1, Ordering::SeqCst);
                        })
                }
            };
            let router = Router::new()
                .route("/_ws_tunnel", get(handler.clone()))
                .route(&format!("/computers/{computer}/_ws_tunnel"), get(handler));
            let (url, job) = serve(router, stop.clone()).await;
            jobs.push(job);
            destinations = vec![url];
        }
        let (endpoint, job) = tls_proxy(&destinations[0], directory, stop.clone()).await;
        jobs.push(job);
        Self {
            endpoint: format!("{endpoint}/computers/{computer}"),
            scoped,
            root,
            active,
            stop,
            jobs,
        }
    }
}
async fn tls_proxy(
    destination: &str,
    directory: &Path,
    stop: CancellationToken,
) -> (String, tokio::task::JoinHandle<()>) {
    let mut ca = CertificateParams::new(vec![]).unwrap();
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca_key = KeyPair::generate().unwrap();
    std::fs::write(
        directory.join("cli-ca.pem"),
        ca.self_signed(&ca_key).unwrap().pem(),
    )
    .unwrap();
    let issuer = Issuer::new(ca, ca_key);
    let mut params = CertificateParams::new(vec!["127.0.0.1".into()]).unwrap();
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let key = KeyPair::generate().unwrap();
    let cert = params.signed_by(&key, &issuer).unwrap();
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let target = destination.strip_prefix("http://").unwrap().to_owned();
    let job = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = stop.cancelled() => return,
                Some(_) = connections.join_next(), if !connections.is_empty() => {},
                accepted = listener.accept() => {
                    let (socket, _) = accepted.unwrap();
                    let acceptor = acceptor.clone();
                    let target = target.clone();
                    connections.spawn(async move {
                        let Ok(Ok(mut tls)) = tokio::time::timeout(Duration::from_secs(5), acceptor.accept(socket)).await else { return; };
                        let Ok(mut upstream) = tokio::net::TcpStream::connect(target).await else { return; };
                        let _ = tokio::io::copy_bidirectional(&mut tls, &mut upstream).await;
                    });
                }
            }
        }
    });
    (format!("https://{address}"), job)
}
