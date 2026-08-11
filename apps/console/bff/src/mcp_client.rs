use std::{
    collections::{BTreeMap, HashMap},
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::Context;
use chrono::Utc;
use rmcp::{
    ClientHandler, ServiceExt,
    model::{
        ClientCapabilities, ClientInfo, Implementation, PaginatedRequestParams, Resource,
        ResourceUpdatedNotificationParam, SubscribeRequestParams, Tool, UnsubscribeRequestParams,
    },
    service::{NotificationContext, RoleClient, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;
use veoveo_mcp_contract::GatewayDiscoveryDegradation;

use crate::{config::Config, outbound_http::OutboundTrust};

/// Console host MCP client: declares the apps extension so servers know an
/// app-capable host is attached; everything else is default client behavior.
#[derive(Clone)]
pub(crate) struct ConsoleHostHandler {
    resource_updates: broadcast::Sender<String>,
    catalog_revision: Arc<AtomicU64>,
}

impl Default for ConsoleHostHandler {
    fn default() -> Self {
        let (resource_updates, _) = broadcast::channel(128);
        Self {
            resource_updates,
            catalog_revision: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl ConsoleHostHandler {
    fn invalidate_catalog(&self) {
        self.catalog_revision.fetch_add(1, Ordering::AcqRel);
    }
}

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

    async fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let _ = self.resource_updates.send(params.uri);
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<RoleClient>) {
        self.invalidate_catalog();
    }

    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        self.invalidate_catalog();
    }
}

type RunningMcpSession = RunningService<rmcp::RoleClient, ConsoleHostHandler>;

/// The App authorization surface projected by one initialized gateway
/// session. MCP list-change notifications invalidate its successful snapshot;
/// partial snapshots retry unavailable servers on the next explicit request.
#[derive(Debug)]
pub(crate) struct McpAppCatalog {
    resources: Vec<rmcp::model::Resource>,
    tools: Vec<rmcp::model::Tool>,
    degradation: GatewayDiscoveryDegradation,
}

impl McpAppCatalog {
    pub(crate) fn resources(&self) -> &[rmcp::model::Resource] {
        &self.resources
    }

    pub(crate) fn tools(&self) -> &[rmcp::model::Tool] {
        &self.tools
    }

    pub(crate) fn degradation(&self) -> &GatewayDiscoveryDegradation {
        &self.degradation
    }
}

struct CachedMcpAppCatalog {
    revision: u64,
    catalog: Arc<McpAppCatalog>,
}

pub(crate) struct McpSessionContext {
    service: Arc<RunningMcpSession>,
    app_catalog: Mutex<Option<CachedMcpAppCatalog>>,
    catalog_revision: Arc<AtomicU64>,
    resource_updates: broadcast::Sender<String>,
    app_resource_subscriptions: Mutex<AppResourceSubscriptions>,
}

#[derive(Default)]
struct AppResourceSubscriptions {
    by_id: BTreeMap<Uuid, String>,
    counts_by_uri: BTreeMap<String, usize>,
}

impl Deref for McpSessionContext {
    type Target = RunningMcpSession;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

impl McpSessionContext {
    pub(crate) async fn app_catalog(&self) -> Result<Arc<McpAppCatalog>, rmcp::ServiceError> {
        let revision = self.catalog_revision.load(Ordering::Acquire);
        let mut cached = self.app_catalog.lock().await;
        if let Some(cached) = cached.as_ref()
            && cached.revision == revision
            && cached.catalog.degradation.is_empty()
        {
            return Ok(cached.catalog.clone());
        }
        let (resources, tools) = tokio::try_join!(
            list_resources_with_degradation(&self.service),
            list_tools_with_degradation(&self.service),
        )?;
        let (resources, mut degradation) = resources;
        let (tools, tool_degradation) = tools;
        degradation.merge(tool_degradation);
        let catalog = Arc::new(McpAppCatalog {
            resources,
            tools,
            degradation,
        });
        *cached = Some(CachedMcpAppCatalog {
            revision,
            catalog: catalog.clone(),
        });
        Ok(catalog)
    }

    /// Register one browser-App subscription on the pooled MCP session.
    /// EventSource reconnects reuse the same UUID and therefore do not add
    /// another upstream subscription or reference count.
    pub(crate) async fn subscribe_app_resource(
        &self,
        subscription_id: Uuid,
        uri: String,
    ) -> anyhow::Result<broadcast::Receiver<String>> {
        let receiver = self.resource_updates.subscribe();
        let mut subscriptions = self.app_resource_subscriptions.lock().await;
        if let Some(existing) = subscriptions.by_id.get(&subscription_id) {
            anyhow::ensure!(
                existing == &uri,
                "app resource subscription identity is already bound to another URI"
            );
            return Ok(receiver);
        }
        let first_for_uri = !subscriptions.counts_by_uri.contains_key(&uri);
        if first_for_uri {
            self.service
                .subscribe(SubscribeRequestParams::new(uri.clone()))
                .await
                .context("subscribing pooled Console MCP session to App resource")?;
        }
        subscriptions.by_id.insert(subscription_id, uri.clone());
        *subscriptions.counts_by_uri.entry(uri).or_default() += 1;
        Ok(receiver)
    }

    /// Release one App subscription. Multiple tabs sharing the same Console
    /// MCP session retain the one upstream subscription until the final UUID
    /// closes.
    pub(crate) async fn unsubscribe_app_resource(
        &self,
        subscription_id: Uuid,
    ) -> anyhow::Result<()> {
        let mut subscriptions = self.app_resource_subscriptions.lock().await;
        let Some(uri) = subscriptions.by_id.get(&subscription_id).cloned() else {
            return Ok(());
        };
        let final_for_uri = subscriptions.counts_by_uri.get(&uri).copied() == Some(1);
        if final_for_uri {
            self.service
                .unsubscribe(UnsubscribeRequestParams::new(uri.clone()))
                .await
                .context("unsubscribing pooled Console MCP session from App resource")?;
        }
        subscriptions.by_id.remove(&subscription_id);
        if final_for_uri {
            subscriptions.counts_by_uri.remove(&uri);
        } else if let Some(count) = subscriptions.counts_by_uri.get_mut(&uri) {
            *count -= 1;
        }
        Ok(())
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
        let handler = ConsoleHostHandler::default();
        let service = Arc::new(
            handler
                .clone()
                .serve(transport)
                .await
                .context("initializing console MCP session to the gateway")?,
        );
        let session = Arc::new(McpSessionContext {
            service,
            app_catalog: Mutex::new(None),
            catalog_revision: handler.catalog_revision,
            resource_updates: handler.resource_updates,
            app_resource_subscriptions: Mutex::new(AppResourceSubscriptions::default()),
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

async fn list_resources_with_degradation(
    service: &RunningMcpSession,
) -> Result<(Vec<Resource>, GatewayDiscoveryDegradation), rmcp::ServiceError> {
    let mut cursor = None;
    let mut resources = Vec::new();
    let mut degradation = GatewayDiscoveryDegradation::default();
    loop {
        let result = service
            .list_resources(Some(PaginatedRequestParams::default().with_cursor(cursor)))
            .await?;
        let page_degradation = GatewayDiscoveryDegradation::from_meta(result.meta.as_ref())
            .map_err(|error| {
                rmcp::ServiceError::McpError(rmcp::ErrorData::internal_error(
                    format!("gateway returned invalid resource discovery metadata: {error}"),
                    None,
                ))
            })?;
        degradation.merge(page_degradation);
        resources.extend(result.resources);
        cursor = result.next_cursor;
        if cursor.is_none() {
            return Ok((resources, degradation));
        }
    }
}

async fn list_tools_with_degradation(
    service: &RunningMcpSession,
) -> Result<(Vec<Tool>, GatewayDiscoveryDegradation), rmcp::ServiceError> {
    let mut cursor = None;
    let mut tools = Vec::new();
    let mut degradation = GatewayDiscoveryDegradation::default();
    loop {
        let result = service
            .list_tools(Some(PaginatedRequestParams::default().with_cursor(cursor)))
            .await?;
        let page_degradation = GatewayDiscoveryDegradation::from_meta(result.meta.as_ref())
            .map_err(|error| {
                rmcp::ServiceError::McpError(rmcp::ErrorData::internal_error(
                    format!("gateway returned invalid tool discovery metadata: {error}"),
                    None,
                ))
            })?;
        degradation.merge(page_degradation);
        tools.extend(result.tools);
        cursor = result.next_cursor;
        if cursor.is_none() {
            return Ok((tools, degradation));
        }
    }
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

    #[test]
    fn list_change_notifications_advance_catalog_revision() {
        let handler = ConsoleHostHandler::default();
        assert_eq!(handler.catalog_revision.load(Ordering::Acquire), 0);
        handler.invalidate_catalog();
        handler.invalidate_catalog();
        assert_eq!(handler.catalog_revision.load(Ordering::Acquire), 2);
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
