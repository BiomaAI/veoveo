use std::{collections::BTreeMap, sync::Arc, time::Duration};

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

pub(crate) type McpSession = Arc<RunningService<rmcp::RoleClient, ConsoleHostHandler>>;

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
        let session: McpSession = Arc::new(
            ConsoleHostHandler
                .serve(transport)
                .await
                .context("initializing console MCP session to the gateway")?,
        );
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

fn fingerprint(access_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(access_token.as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
