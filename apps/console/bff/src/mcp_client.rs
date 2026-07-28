use std::{collections::BTreeMap, ops::Deref, sync::Arc, time::Duration};

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

use crate::config::Config;

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
/// snapshot; an explicit Apps listing refreshes the snapshot for the same
/// session, while token rotation or transport invalidation drops it.
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
        self.load_app_catalog(false).await
    }

    pub(crate) async fn refresh_app_catalog(
        &self,
    ) -> Result<Arc<McpAppCatalog>, rmcp::ServiceError> {
        self.load_app_catalog(true).await
    }

    async fn load_app_catalog(
        &self,
        refresh: bool,
    ) -> Result<Arc<McpAppCatalog>, rmcp::ServiceError> {
        cached_or_refresh(&self.app_catalog, refresh, || async {
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
    pub(crate) fn new() -> anyhow::Result<Self> {
        // The MCP stream outlives ordinary request timeouts; only connection
        // establishment is bounded.
        let http = reqwest::Client::builder()
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
        let transport = StreamableHttpClientTransport::<reqwest::Client>::with_client(
            self.http.clone(),
            StreamableHttpClientTransportConfig::with_uri(config.oauth_resource().to_string())
                .auth_header(access_token.to_owned())
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

async fn cached_or_refresh<T, E, F, Fut>(
    cache: &Mutex<Option<Arc<T>>>,
    refresh: bool,
    load: F,
) -> Result<Arc<T>, E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut cached = cache.lock().await;
    if !refresh && let Some(value) = cached.as_ref() {
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

    use super::*;

    #[tokio::test]
    async fn session_cache_loads_once_until_explicit_refresh() {
        let cache = Mutex::new(None);
        let loads = AtomicUsize::new(0);

        let first = cached_or_refresh(&cache, false, || async {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(41)
        })
        .await
        .unwrap();
        let cached = cached_or_refresh(&cache, false, || async {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(42)
        })
        .await
        .unwrap();
        let refreshed = cached_or_refresh(&cache, true, || async {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(43)
        })
        .await
        .unwrap();

        assert!(Arc::ptr_eq(&first, &cached));
        assert_eq!(*refreshed, 43);
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn failed_catalog_load_is_not_cached() {
        let cache = Mutex::new(None);

        assert_eq!(
            cached_or_refresh(&cache, false, || async {
                Err::<usize, _>("catalog unavailable")
            })
            .await
            .unwrap_err(),
            "catalog unavailable"
        );
        assert_eq!(
            *cached_or_refresh(&cache, false, || async { Ok::<_, &str>(7) })
                .await
                .unwrap(),
            7
        );
    }
}
