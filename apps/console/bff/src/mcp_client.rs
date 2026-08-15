use std::{
    collections::{BTreeMap, HashMap},
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::Context;
use chrono::Utc;
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt,
    model::{
        ClientCapabilities, ClientInfo, Implementation, PaginatedRequestParams, Resource,
        ResourceUpdatedNotificationParam, ServerNotification, SubscriptionFilter, Tool,
    },
    service::{NotificationContext, RoleClient, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, broadcast, oneshot};
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
        capabilities
            .extensions
            .get_or_insert_default()
            .entry(rmcp::model::TASKS_EXTENSION_ID.to_owned())
            .or_default();
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

type RunningMcpClient = RunningService<rmcp::RoleClient, ConsoleHostHandler>;

/// The App authorization surface projected by one auth-scoped gateway client.
/// MCP list-change notifications invalidate its successful snapshot;
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

    #[cfg(test)]
    pub(crate) fn for_test(resources: Vec<Resource>, tools: Vec<Tool>) -> Self {
        Self {
            resources,
            tools,
            degradation: GatewayDiscoveryDegradation::default(),
        }
    }
}

struct CachedMcpAppCatalog {
    revision: u64,
    catalog: Arc<McpAppCatalog>,
}

pub(crate) struct AuthScopedMcpClient {
    service: Arc<RunningMcpClient>,
    app_catalog: Mutex<Option<CachedMcpAppCatalog>>,
    catalog_revision: Arc<AtomicU64>,
    resource_updates: broadcast::Sender<String>,
    app_resource_subscriptions: Mutex<AppResourceSubscriptions>,
    app_resource_subscription_locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    app_resource_capacity: AppResourceCapacity,
    shutting_down: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AppResourceCapacity {
    pub(crate) max_upstream_listeners: usize,
    pub(crate) max_downstream_subscriptions: usize,
}

impl Default for AppResourceCapacity {
    fn default() -> Self {
        Self {
            max_upstream_listeners: 64,
            max_downstream_subscriptions: 256,
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppResourceSubscription {
    pub(crate) receiver: broadcast::Receiver<String>,
    pub(crate) newly_registered: bool,
}

#[derive(Default)]
struct AppResourceSubscriptions {
    by_id: BTreeMap<Uuid, String>,
    pending_by_id: BTreeMap<Uuid, String>,
    counts_by_uri: BTreeMap<String, usize>,
    pending_uris: std::collections::BTreeSet<String>,
    listeners_by_uri: BTreeMap<String, AppResourceListener>,
}

#[derive(Debug)]
pub(crate) enum AppResourceSubscriptionError {
    Capacity {
        resource: &'static str,
        limit: usize,
    },
    IdentityConflict,
    ClientClosing,
    Upstream(anyhow::Error),
}

impl AppResourceSubscriptionError {
    #[cfg(test)]
    pub(crate) const fn capacity(&self) -> Option<(&'static str, usize)> {
        match self {
            Self::Capacity { resource, limit } => Some((resource, *limit)),
            _ => None,
        }
    }
}

impl std::fmt::Display for AppResourceSubscriptionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Capacity { resource, limit } => {
                write!(
                    formatter,
                    "App resource {resource} capacity {limit} is exhausted"
                )
            }
            Self::IdentityConflict => formatter
                .write_str("App resource subscription identity is already bound to another URI"),
            Self::ClientClosing => formatter.write_str("auth-scoped MCP client is closing"),
            Self::Upstream(error) => write!(formatter, "{error:#}"),
        }
    }
}

impl std::error::Error for AppResourceSubscriptionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Upstream(error) => error.source(),
            _ => None,
        }
    }
}

struct AppResourceListener {
    cancel: oneshot::Sender<()>,
    stopped: oneshot::Receiver<()>,
}

impl Deref for AuthScopedMcpClient {
    type Target = RunningMcpClient;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

impl AuthScopedMcpClient {
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

    /// Register one browser-App subscription on the auth-scoped MCP client.
    /// EventSource reconnects reuse the same UUID and therefore do not add
    /// another upstream subscription or reference count.
    pub(crate) async fn subscribe_app_resource(
        &self,
        subscription_id: Uuid,
        uri: String,
    ) -> Result<AppResourceSubscription, AppResourceSubscriptionError> {
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(AppResourceSubscriptionError::ClientClosing);
        }
        let receiver = self.resource_updates.subscribe();
        let uri_lock = self.app_resource_subscription_lock(&uri).await;
        let _uri_guard = uri_lock.lock().await;
        let mut subscriptions = self.app_resource_subscriptions.lock().await;
        if let Some(existing) = subscriptions.by_id.get(&subscription_id) {
            if existing != &uri {
                return Err(AppResourceSubscriptionError::IdentityConflict);
            }
            return Ok(AppResourceSubscription {
                receiver,
                newly_registered: false,
            });
        }
        if subscriptions.pending_by_id.contains_key(&subscription_id) {
            return Err(AppResourceSubscriptionError::IdentityConflict);
        }
        if subscriptions.by_id.len() + subscriptions.pending_by_id.len()
            >= self.app_resource_capacity.max_downstream_subscriptions
        {
            return Err(AppResourceSubscriptionError::Capacity {
                resource: "downstream_subscriptions",
                limit: self.app_resource_capacity.max_downstream_subscriptions,
            });
        }
        let first_for_uri = !subscriptions.counts_by_uri.contains_key(&uri);
        if !first_for_uri {
            subscriptions.by_id.insert(subscription_id, uri.clone());
            *subscriptions.counts_by_uri.entry(uri).or_default() += 1;
            return Ok(AppResourceSubscription {
                receiver,
                newly_registered: true,
            });
        }
        if subscriptions.listeners_by_uri.len() + subscriptions.pending_uris.len()
            >= self.app_resource_capacity.max_upstream_listeners
        {
            return Err(AppResourceSubscriptionError::Capacity {
                resource: "upstream_listeners",
                limit: self.app_resource_capacity.max_upstream_listeners,
            });
        }
        subscriptions
            .pending_by_id
            .insert(subscription_id, uri.clone());
        subscriptions.pending_uris.insert(uri.clone());
        drop(subscriptions);
        let filter = SubscriptionFilter::builder()
            .resource_subscription(uri.clone())
            .build();
        let listener = self
            .service
            .listen(filter)
            .await
            .context("opening Console App resource listener");
        let mut listener = match listener {
            Ok(listener) => listener,
            Err(error) => {
                let mut subscriptions = self.app_resource_subscriptions.lock().await;
                subscriptions.pending_by_id.remove(&subscription_id);
                subscriptions.pending_uris.remove(&uri);
                return Err(AppResourceSubscriptionError::Upstream(error));
            }
        };
        let (cancel_tx, mut cancel_rx) = oneshot::channel();
        let (stopped_tx, stopped_rx) = oneshot::channel();
        let resource_updates = self.resource_updates.clone();
        let catalog_revision = self.catalog_revision.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut cancel_rx => {
                        let _ = listener.cancel().await;
                        break;
                    }
                    notification = listener.next() => match notification {
                        Ok(Some(ServerNotification::ResourceUpdatedNotification(update))) => {
                            let _ = resource_updates.send(update.params.uri);
                        }
                        Ok(Some(ServerNotification::ResourceListChangedNotification(_))) => {
                            catalog_revision.fetch_add(1, Ordering::AcqRel);
                        }
                        Ok(Some(_)) => {}
                        Ok(None) | Err(_) => break,
                    }
                }
            }
            let _ = stopped_tx.send(());
        });
        subscriptions = self.app_resource_subscriptions.lock().await;
        subscriptions.pending_by_id.remove(&subscription_id);
        subscriptions.pending_uris.remove(&uri);
        if self.shutting_down.load(Ordering::Acquire) {
            drop(subscriptions);
            Self::stop_app_resource_listener(AppResourceListener {
                cancel: cancel_tx,
                stopped: stopped_rx,
            })
            .await
            .map_err(AppResourceSubscriptionError::Upstream)?;
            return Err(AppResourceSubscriptionError::ClientClosing);
        }
        if let Some(existing) = subscriptions.by_id.get(&subscription_id) {
            let same_uri = existing == &uri;
            drop(subscriptions);
            Self::stop_app_resource_listener(AppResourceListener {
                cancel: cancel_tx,
                stopped: stopped_rx,
            })
            .await
            .map_err(AppResourceSubscriptionError::Upstream)?;
            if !same_uri {
                return Err(AppResourceSubscriptionError::IdentityConflict);
            }
            return Ok(AppResourceSubscription {
                receiver,
                newly_registered: false,
            });
        }
        subscriptions.listeners_by_uri.insert(
            uri.clone(),
            AppResourceListener {
                cancel: cancel_tx,
                stopped: stopped_rx,
            },
        );
        subscriptions.by_id.insert(subscription_id, uri.clone());
        *subscriptions.counts_by_uri.entry(uri).or_default() += 1;
        Ok(AppResourceSubscription {
            receiver,
            newly_registered: true,
        })
    }

    async fn shutdown(&self) {
        if self.shutting_down.swap(true, Ordering::AcqRel) {
            return;
        }
        let listeners = {
            let mut subscriptions = self.app_resource_subscriptions.lock().await;
            subscriptions.by_id.clear();
            subscriptions.pending_by_id.clear();
            subscriptions.counts_by_uri.clear();
            subscriptions.pending_uris.clear();
            std::mem::take(&mut subscriptions.listeners_by_uri)
                .into_values()
                .collect::<Vec<_>>()
        };
        futures::future::join_all(listeners.into_iter().map(Self::stop_app_resource_listener))
            .await;
        self.service.cancellation_token().cancel();
    }

    /// Release one App subscription. Multiple tabs sharing the same Console
    /// MCP client retain the one upstream subscription until the final UUID
    /// closes.
    pub(crate) async fn unsubscribe_app_resource(
        &self,
        subscription_id: Uuid,
    ) -> anyhow::Result<()> {
        let Some(uri) = self
            .app_resource_subscriptions
            .lock()
            .await
            .by_id
            .get(&subscription_id)
            .cloned()
        else {
            return Ok(());
        };
        let uri_lock = self.app_resource_subscription_lock(&uri).await;
        let _uri_guard = uri_lock.lock().await;
        let mut subscriptions = self.app_resource_subscriptions.lock().await;
        let Some(current_uri) = subscriptions.by_id.get(&subscription_id) else {
            return Ok(());
        };
        anyhow::ensure!(
            current_uri == &uri,
            "app resource subscription identity changed URI while unsubscribing"
        );
        let final_for_uri = subscriptions.counts_by_uri.get(&uri).copied() == Some(1);
        let listener = final_for_uri
            .then(|| subscriptions.listeners_by_uri.remove(&uri))
            .flatten();
        subscriptions.by_id.remove(&subscription_id);
        if final_for_uri {
            subscriptions.counts_by_uri.remove(&uri);
        } else if let Some(count) = subscriptions.counts_by_uri.get_mut(&uri) {
            *count -= 1;
        }
        drop(subscriptions);
        if let Some(listener) = listener {
            Self::stop_app_resource_listener(listener).await?;
        }
        Ok(())
    }

    async fn stop_app_resource_listener(listener: AppResourceListener) -> anyhow::Result<()> {
        let _ = listener.cancel.send(());
        tokio::time::timeout(Duration::from_secs(2), listener.stopped)
            .await
            .context("timed out stopping Console App resource listener")?
            .context("Console App resource listener stopped without acknowledgement")
    }

    async fn app_resource_subscription_lock(&self, uri: &str) -> Arc<Mutex<()>> {
        self.app_resource_subscription_locks
            .lock()
            .await
            .entry(uri.to_owned())
            .or_default()
            .clone()
    }
}

pub(crate) type SharedMcpClient = Arc<AuthScopedMcpClient>;

struct CachedClient {
    client: SharedMcpClient,
    access_fingerprint: String,
    expires_at: i64,
}

/// One gateway MCP client per browser-session auth scope. Access-token
/// replacement swaps the client under that stable scope and shuts down its
/// listeners before opening the new transport. Tokens are never retained as
/// map keys.
pub(crate) struct AuthScopedMcpClientPool {
    http: reqwest::Client,
    clients: Mutex<BTreeMap<String, CachedClient>>,
    app_resource_capacity: AppResourceCapacity,
}

const SESSION_EXPIRY_MARGIN_SECS: i64 = 5;

impl AuthScopedMcpClientPool {
    #[cfg(test)]
    pub(crate) fn new(outbound_trust: &OutboundTrust) -> anyhow::Result<Self> {
        Self::new_with_capacity(outbound_trust, AppResourceCapacity::default())
    }

    pub(crate) fn new_with_capacity(
        outbound_trust: &OutboundTrust,
        app_resource_capacity: AppResourceCapacity,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            app_resource_capacity.max_upstream_listeners > 0
                && app_resource_capacity.max_downstream_subscriptions > 0,
            "App resource capacities must be positive"
        );
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
            clients: Mutex::new(BTreeMap::new()),
            app_resource_capacity,
        })
    }

    pub(crate) async fn client(
        &self,
        config: &Config,
        access_token: &str,
        access_expires_at: i64,
        auth_scope: &str,
    ) -> anyhow::Result<SharedMcpClient> {
        let key = fingerprint(auth_scope);
        let access_fingerprint = fingerprint(access_token);
        let now = Utc::now().timestamp();
        let mut clients = self.clients.lock().await;
        let expired = clients
            .extract_if(.., |_, cached| {
                cached.expires_at <= now + SESSION_EXPIRY_MARGIN_SECS
            })
            .map(|(_, cached)| cached.client)
            .collect::<Vec<_>>();
        for stale in expired {
            stale.shutdown().await;
        }
        if let Some(cached) = clients.get(&key)
            && cached.access_fingerprint == access_fingerprint
        {
            return Ok(cached.client.clone());
        }
        if let Some(stale) = clients.remove(&key) {
            stale.client.shutdown().await;
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
                .custom_headers(transport_headers),
        );
        let handler = ConsoleHostHandler::default();
        let service = Arc::new(
            handler
                .clone()
                .serve_with_lifecycle(
                    transport,
                    ClientLifecycleMode::Discover {
                        preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                    },
                )
                .await
                .context("starting auth-scoped Console MCP client")?,
        );
        let client = Arc::new(AuthScopedMcpClient {
            service,
            app_catalog: Mutex::new(None),
            catalog_revision: handler.catalog_revision,
            resource_updates: handler.resource_updates,
            app_resource_subscriptions: Mutex::new(AppResourceSubscriptions::default()),
            app_resource_subscription_locks: Mutex::new(BTreeMap::new()),
            app_resource_capacity: self.app_resource_capacity,
            shutting_down: AtomicBool::new(false),
        });
        clients.insert(
            key,
            CachedClient {
                client: client.clone(),
                access_fingerprint,
                expires_at: access_expires_at,
            },
        );
        Ok(client)
    }

    /// Drop `stale` from the pool if it is still the client cached for this
    /// browser-session auth scope, so the next call creates a fresh transport.
    pub(crate) async fn invalidate(&self, auth_scope: &str, stale: &SharedMcpClient) {
        let key = fingerprint(auth_scope);
        let mut clients = self.clients.lock().await;
        if let Some(cached) = clients.get(&key)
            && Arc::ptr_eq(&cached.client, stale)
        {
            let cached = clients.remove(&key).expect("entry observed under lock");
            cached.client.shutdown().await;
        }
    }
}

async fn list_resources_with_degradation(
    service: &RunningMcpClient,
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
    service: &RunningMcpClient,
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
    use std::sync::atomic::AtomicUsize;

    use axum::Router;
    use rmcp::{
        ErrorData as McpError, ServerHandler,
        model::{ServerCapabilities, ServerInfo, SubscriptionFilter},
        service::SubscriptionContext,
        transport::streamable_http_server::{
            StreamableHttpService, session::never::NeverSessionManager,
        },
    };
    use tokio::{sync::Semaphore, task::JoinHandle};
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

    struct SubscriptionProbe {
        subscribe_calls: AtomicUsize,
        unsubscribe_calls: AtomicUsize,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        release: Semaphore,
    }

    impl Default for SubscriptionProbe {
        fn default() -> Self {
            Self {
                subscribe_calls: AtomicUsize::new(0),
                unsubscribe_calls: AtomicUsize::new(0),
                in_flight: AtomicUsize::new(0),
                max_in_flight: AtomicUsize::new(0),
                release: Semaphore::new(0),
            }
        }
    }

    #[derive(Clone, Default)]
    struct DelayedSubscriptionMcp {
        probe: Arc<SubscriptionProbe>,
    }

    impl ServerHandler for DelayedSubscriptionMcp {
        fn get_info(&self) -> ServerInfo {
            ServerInfo::new(
                ServerCapabilities::builder()
                    .enable_resources()
                    .enable_resources_subscribe()
                    .build(),
            )
        }

        fn accepted_subscription_filter(
            &self,
            requested: &SubscriptionFilter,
        ) -> Option<SubscriptionFilter> {
            Some(requested.clone())
        }

        async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
            self.probe.subscribe_calls.fetch_add(1, Ordering::SeqCst);
            let in_flight = self.probe.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.probe
                .max_in_flight
                .fetch_max(in_flight, Ordering::SeqCst);
            tokio::select! {
                permit = self.probe.release.acquire() => {
                    permit.expect("subscription test release remains open").forget();
                }
                () = context.cancelled() => {}
            }
            self.probe.in_flight.fetch_sub(1, Ordering::SeqCst);
            self.probe.unsubscribe_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    async fn subscription_test_session(
        handler: DelayedSubscriptionMcp,
    ) -> (SharedMcpClient, JoinHandle<()>) {
        subscription_test_session_with_capacity(handler, AppResourceCapacity::default()).await
    }

    async fn subscription_test_session_with_capacity(
        handler: DelayedSubscriptionMcp,
        capacity: AppResourceCapacity,
    ) -> (SharedMcpClient, JoinHandle<()>) {
        let (_, _, session, server) = subscription_test_pool(handler, capacity).await;
        (session, server)
    }

    async fn subscription_test_pool(
        handler: DelayedSubscriptionMcp,
        capacity: AppResourceCapacity,
    ) -> (
        AuthScopedMcpClientPool,
        Config,
        SharedMcpClient,
        JoinHandle<()>,
    ) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind subscription test MCP");
        let address = listener
            .local_addr()
            .expect("subscription test MCP address");
        let service: StreamableHttpService<DelayedSubscriptionMcp, NeverSessionManager> =
            StreamableHttpService::new(
                move || Ok(handler.clone()),
                veoveo_mcp_contract::stateless_session_manager(),
                veoveo_mcp_contract::canonical_streamable_http_server_config(),
            );
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().nest_service("/mcp/admin", service))
                .await
                .expect("serve subscription test MCP");
        });
        let gateway_url = Url::parse(&format!("http://{address}")).unwrap();
        let config = Config::for_test(gateway_url);
        let pool = AuthScopedMcpClientPool::new_with_capacity(&OutboundTrust::default(), capacity)
            .unwrap();
        let session = pool
            .client(
                &config,
                "subscription-test-access-token",
                Utc::now().timestamp() + 60,
                "subscription-test-auth-scope",
            )
            .await
            .expect("connect subscription test MCP client");
        (pool, config, session, server)
    }

    async fn wait_for_subscribe_calls(probe: &SubscriptionProbe, expected: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while probe.subscribe_calls.load(Ordering::SeqCst) < expected {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("expected upstream subscription calls");
    }

    async fn wait_for_unsubscribe_calls(probe: &SubscriptionProbe, expected: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while probe.unsubscribe_calls.load(Ordering::SeqCst) < expected {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("expected upstream listener cancellation");
    }

    #[tokio::test]
    async fn distinct_app_resources_subscribe_concurrently() {
        let handler = DelayedSubscriptionMcp::default();
        let probe = handler.probe.clone();
        let (session, server) = subscription_test_session(handler).await;
        let first_session = session.clone();
        let first = tokio::spawn(async move {
            first_session
                .subscribe_app_resource(Uuid::now_v7(), "fleet://plans".to_owned())
                .await
        });
        let second_session = session.clone();
        let second = tokio::spawn(async move {
            second_session
                .subscribe_app_resource(Uuid::now_v7(), "fleet://objectives".to_owned())
                .await
        });

        wait_for_subscribe_calls(&probe, 2).await;
        assert_eq!(probe.max_in_flight.load(Ordering::SeqCst), 2);
        probe.release.add_permits(2);
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();

        session.cancellation_token().cancel();
        server.abort();
    }

    #[tokio::test]
    async fn same_app_resource_reuses_one_upstream_subscription() {
        let handler = DelayedSubscriptionMcp::default();
        let probe = handler.probe.clone();
        let (session, server) = subscription_test_session(handler).await;
        let first_id = Uuid::now_v7();
        let second_id = Uuid::now_v7();
        let first_session = session.clone();
        let first = tokio::spawn(async move {
            first_session
                .subscribe_app_resource(first_id, "fleet://plans".to_owned())
                .await
        });
        wait_for_subscribe_calls(&probe, 1).await;
        let second_session = session.clone();
        let second = tokio::spawn(async move {
            second_session
                .subscribe_app_resource(second_id, "fleet://plans".to_owned())
                .await
        });
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(probe.subscribe_calls.load(Ordering::SeqCst), 1);
        probe.release.add_permits(1);
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();

        session.unsubscribe_app_resource(first_id).await.unwrap();
        assert_eq!(probe.unsubscribe_calls.load(Ordering::SeqCst), 0);
        session.unsubscribe_app_resource(second_id).await.unwrap();
        assert_eq!(probe.unsubscribe_calls.load(Ordering::SeqCst), 1);

        session.cancellation_token().cancel();
        server.abort();
    }

    #[tokio::test]
    async fn app_resource_capacity_rejects_only_the_excess_subscription() {
        let handler = DelayedSubscriptionMcp::default();
        let probe = handler.probe.clone();
        let capacity = AppResourceCapacity {
            max_upstream_listeners: 1,
            max_downstream_subscriptions: 2,
        };
        let (session, server) = subscription_test_session_with_capacity(handler, capacity).await;
        let first_id = Uuid::now_v7();
        let second_id = Uuid::now_v7();
        let first_session = session.clone();
        let first = tokio::spawn(async move {
            first_session
                .subscribe_app_resource(first_id, "fleet://plans".to_owned())
                .await
        });
        wait_for_subscribe_calls(&probe, 1).await;
        let second = session
            .subscribe_app_resource(second_id, "fleet://plans".to_owned())
            .await
            .expect("same URI shares the admitted listener");
        assert!(second.newly_registered);

        let downstream_error = session
            .subscribe_app_resource(Uuid::now_v7(), "fleet://plans".to_owned())
            .await
            .expect_err("downstream capacity must fail closed");
        assert_eq!(
            downstream_error.capacity(),
            Some(("downstream_subscriptions", 2))
        );
        session.unsubscribe_app_resource(second_id).await.unwrap();

        let upstream_error = session
            .subscribe_app_resource(Uuid::now_v7(), "fleet://objectives".to_owned())
            .await
            .expect_err("upstream capacity must fail closed");
        assert_eq!(upstream_error.capacity(), Some(("upstream_listeners", 1)));
        assert_eq!(probe.subscribe_calls.load(Ordering::SeqCst), 1);

        probe.release.add_permits(1);
        first.await.unwrap().unwrap();
        session.unsubscribe_app_resource(first_id).await.unwrap();
        assert_eq!(probe.unsubscribe_calls.load(Ordering::SeqCst), 1);

        session.cancellation_token().cancel();
        server.abort();
    }

    #[tokio::test]
    async fn replacing_an_access_token_cancels_the_old_auth_scope_listeners() {
        let handler = DelayedSubscriptionMcp::default();
        let probe = handler.probe.clone();
        let (pool, config, old_client, server) =
            subscription_test_pool(handler, AppResourceCapacity::default()).await;
        let subscription_id = Uuid::now_v7();
        let subscriber = old_client.clone();
        let subscription = tokio::spawn(async move {
            subscriber
                .subscribe_app_resource(subscription_id, "fleet://plans".to_owned())
                .await
        });
        wait_for_subscribe_calls(&probe, 1).await;
        probe.release.add_permits(1);
        subscription.await.unwrap().unwrap();

        let replacement = pool
            .client(
                &config,
                "replacement-access-token",
                Utc::now().timestamp() + 60,
                "subscription-test-auth-scope",
            )
            .await
            .expect("replace access-token generation");
        assert!(!Arc::ptr_eq(&old_client, &replacement));
        wait_for_unsubscribe_calls(&probe, 1).await;
        assert!(matches!(
            old_client
                .subscribe_app_resource(Uuid::now_v7(), "fleet://plans".to_owned())
                .await,
            Err(AppResourceSubscriptionError::ClientClosing)
        ));

        replacement.cancellation_token().cancel();
        server.abort();
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
        let service: StreamableHttpService<PrivateCaMcp, NeverSessionManager> =
            StreamableHttpService::new(
                || Ok(PrivateCaMcp),
                veoveo_mcp_contract::stateless_session_manager(),
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
        let pool = AuthScopedMcpClientPool::new(&trust).unwrap();
        let session = pool
            .client(
                &config,
                "private-ca-access-token",
                Utc::now().timestamp() + 60,
                "private-ca-auth-scope",
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
