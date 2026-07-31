use std::{
    collections::{BTreeMap, HashMap},
    ops::Deref,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use chrono::Utc;
use rmcp::{
    ClientHandler, ServiceExt,
    model::{ClientCapabilities, ClientInfo, Implementation},
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{config::Config, outbound_http::OutboundTrust};

/// Console host MCP client: declares the apps extension so servers know an
/// app-capable host is attached; everything else is default client behavior.
#[derive(Clone, Default)]
pub(crate) struct ConsoleHostHandler;

impl ClientHandler for ConsoleHostHandler {
    fn get_info(&self) -> ClientInfo {
        let mut capabilities = ClientCapabilities::default();
        let (id, declaration) = veoveo_mcp_apps_extension::host_extension_capability();
        capabilities
            .extensions
            .get_or_insert_default()
            .insert(id, declaration);
        ClientInfo::new(
            capabilities,
            Implementation::new("veoveo-console", env!("CARGO_PKG_VERSION")),
        )
    }
}

type RunningMcpSession = RunningService<rmcp::RoleClient, ConsoleHostHandler>;

/// The App authorization surface projected by one initialized gateway
/// session. Resources and tools are immutable for that authority/profile
/// snapshot. Token rotation or transport invalidation drops it.
#[derive(Debug)]
pub(crate) struct McpAppCatalog {
    resources: Vec<rmcp::model::Resource>,
    tools: Vec<rmcp::model::Tool>,
}

impl McpAppCatalog {
    pub(crate) fn resources(&self) -> &[rmcp::model::Resource] {
        &self.resources
    }

    pub(crate) fn tools(&self) -> &[rmcp::model::Tool] {
        &self.tools
    }
}

pub(crate) struct McpSessionContext {
    service: Arc<RunningMcpSession>,
    app_catalog: Mutex<Option<Arc<McpAppCatalog>>>,
}

impl Deref for McpSessionContext {
    type Target = RunningMcpSession;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

impl McpSessionContext {
    pub(crate) async fn app_catalog(&self) -> Result<Arc<McpAppCatalog>, rmcp::ServiceError> {
        cached_or_load(&self.app_catalog, || async {
            let resources = self.service.list_all_resources().await?;
            let tools = self.service.list_all_tools().await?;
            Ok(McpAppCatalog { resources, tools })
        })
        .await
    }
}

pub(crate) type McpSession = Arc<McpSessionContext>;

struct CachedSession {
    session: McpSession,
    expires_at: i64,
}

/// One MCP session to the gateway per browser session per token generation,
/// keyed by an access-token fingerprint (the token itself is never stored).
/// Token refresh rolls to a new key; expired entries are swept on access, so
/// a signed-out session dies with its token TTL.
pub(crate) struct McpSessionPool {
    http: reqwest::Client,
    sessions: Mutex<BTreeMap<String, CachedSession>>,
}

const SESSION_EXPIRY_MARGIN_SECS: i64 = 5;

impl McpSessionPool {
    pub(crate) fn new(outbound_trust: &OutboundTrust) -> anyhow::Result<Self> {
        // The MCP stream outlives ordinary request timeouts; only connection
        // establishment is bounded.
        let http = outbound_trust
            .client_builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("building console MCP HTTP client")?;
        Ok(Self {
            http,
            sessions: Mutex::new(BTreeMap::new()),
        })
    }

    pub(crate) async fn session(
        &self,
        config: &Config,
        access_token: &str,
        access_expires_at: i64,
    ) -> anyhow::Result<McpSession> {
        let key = fingerprint(access_token);
        let now = Utc::now().timestamp();
        let mut sessions = self.sessions.lock().await;
        for (_, stale) in sessions.extract_if(.., |_, cached| {
            cached.expires_at <= now + SESSION_EXPIRY_MARGIN_SECS
        }) {
            stale.session.cancellation_token().cancel();
        }
        if let Some(cached) = sessions.get(&key) {
            return Ok(cached.session.clone());
        }
        let mut transport_headers = HashMap::new();
        transport_headers.insert(
            reqwest::header::HOST,
            config
                .gateway_host()
                .parse()
                .context("PUBLIC_BASE_URL authority is not a valid HTTP Host header")?,
        );
        let transport = StreamableHttpClientTransport::<reqwest::Client>::with_client(
            self.http.clone(),
            StreamableHttpClientTransportConfig::with_uri(config.mcp_transport_url().to_string())
                .auth_header(access_token.to_owned())
                // The internal transport address is not the gateway's public
                // authority. Preserve that authority for gateway Host checks.
                .custom_headers(transport_headers)
                // The gateway keeps MCP sessions in memory; a gateway restart
                // discards them all while this pool still holds the old
                // session ID. Let rmcp redo the handshake and replay the
                // failed request instead of pinning HTTP 404 until the
                // access token rotates.
                .reinit_on_expired_session(true),
        );
        let service = Arc::new(
            ConsoleHostHandler
                .serve(transport)
                .await
                .context("initializing console MCP session to the gateway")?,
        );
        let session = Arc::new(McpSessionContext {
            service,
            app_catalog: Mutex::new(None),
        });
        sessions.insert(
            key,
            CachedSession {
                session: session.clone(),
                expires_at: access_expires_at,
            },
        );
        Ok(session)
    }

    /// Drop `stale` from the pool (if it is still the cached entry for this
    /// token) so the next `session` call builds a fresh one. Used after a
    /// transport-level failure that outlived rmcp's own single-attempt
    /// expired-session recovery, e.g. a session whose worker task has died.
    pub(crate) async fn invalidate(&self, access_token: &str, stale: &McpSession) {
        let key = fingerprint(access_token);
        let mut sessions = self.sessions.lock().await;
        if let Some(cached) = sessions.get(&key)
            && Arc::ptr_eq(&cached.session, stale)
        {
            let cached = sessions.remove(&key).expect("entry observed under lock");
            cached.session.cancellation_token().cancel();
        }
    }
}

async fn cached_or_load<T, E, F, Fut>(cache: &Mutex<Option<Arc<T>>>, load: F) -> Result<Arc<T>, E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut cached = cache.lock().await;
    if let Some(value) = cached.as_ref() {
        return Ok(value.clone());
    }
    let value = Arc::new(load().await?);
    *cached = Some(value.clone());
    Ok(value)
}

fn fingerprint(access_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(access_token.as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use rmcp::{
        ServerHandler,
        model::{ServerCapabilities, ServerInfo},
        transport::streamable_http_server::{
            StreamableHttpService, session::local::LocalSessionManager,
        },
    };
    use url::Url;

    use super::*;

    #[tokio::test]
    async fn session_cache_loads_once() {
        let cache = Mutex::new(None);
        let loads = AtomicUsize::new(0);

        let first = cached_or_load(&cache, || async {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(41)
        })
        .await
        .unwrap();
        let cached = cached_or_load(&cache, || async {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(42)
        })
        .await
        .unwrap();

        assert!(Arc::ptr_eq(&first, &cached));
        assert_eq!(*cached, 41);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_catalog_load_is_not_cached() {
        let cache = Mutex::new(None);

        assert_eq!(
            cached_or_load(&cache, || async { Err::<usize, _>("catalog unavailable") })
                .await
                .unwrap_err(),
            "catalog unavailable"
        );
        assert_eq!(
            *cached_or_load(&cache, || async { Ok::<_, &str>(7) })
                .await
                .unwrap(),
            7
        );
    }

    #[derive(Clone, Default)]
    struct PrivateCaMcp;

    impl ServerHandler for PrivateCaMcp {
        fn get_info(&self) -> ServerInfo {
            ServerInfo::new(
                ServerCapabilities::builder()
                    .enable_resources()
                    .enable_tools()
                    .build(),
            )
        }
    }

    #[tokio::test]
    async fn private_ca_https_transport_uses_internal_uri_and_public_authority() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let certified_key = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()])
            .expect("private test CA and server certificate");
        let certificate_pem = certified_key.cert.pem();
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem(
            certificate_pem.clone().into_bytes(),
            certified_key.signing_key.serialize_pem().into_bytes(),
        )
        .await
        .expect("private CA TLS configuration");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind private CA MCP");
        listener
            .set_nonblocking(true)
            .expect("nonblocking private CA MCP listener");
        let address = listener.local_addr().expect("private CA MCP address");
        let service: StreamableHttpService<PrivateCaMcp, LocalSessionManager> =
            StreamableHttpService::new(
                || Ok(PrivateCaMcp),
                LocalSessionManager::default().into(),
                veoveo_mcp_contract::canonical_streamable_http_server_config()
                    .with_allowed_hosts(vec!["console.example".to_owned()]),
            );
        let handle = axum_server::Handle::new();
        let server_handle = handle.clone();
        let server = tokio::spawn(async move {
            axum_server::from_tcp_rustls(listener, tls)
                .expect("private CA MCP server")
                .handle(server_handle)
                .serve(
                    Router::new()
                        .nest_service("/mcp/admin", service)
                        .into_make_service(),
                )
                .await
        });

        let public_url = Url::parse("https://console.example").unwrap();
        let internal_url = Url::parse(&format!("https://{address}/mcp/admin")).unwrap();
        let config = Config::for_test(public_url).with_mcp_transport_url(internal_url);
        let trust = OutboundTrust::for_test_pem_bundle(certificate_pem.as_bytes()).unwrap();
        let pool = McpSessionPool::new(&trust).unwrap();
        let session = pool
            .session(
                &config,
                "private-ca-access-token",
                Utc::now().timestamp() + 60,
            )
            .await
            .expect("MCP initialization through installation CA and internal transport");
        let catalog = session.app_catalog().await.expect("empty MCP app catalog");
        assert!(catalog.resources().is_empty());
        assert!(catalog.tools().is_empty());

        session.cancellation_token().cancel();
        handle.shutdown();
        server.await.unwrap().unwrap();
    }
}
