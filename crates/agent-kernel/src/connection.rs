//! The agent's gateway session: identity, transport, and rotation.
//!
//! The agent is its own OAuth principal. Each session mints a gateway access
//! token via the client-credentials grant with a private-key JWT client
//! assertion (RFC 7523), attaches it as the streamable-HTTP `auth_header`,
//! and connects rig's `McpClientHandler` so gateway tools land on the shared
//! `ToolServerHandle`.
//!
//! rmcp fixes the auth header at transport construction, so token refresh is
//! connection rotation: mint → connect the replacement → publish the new
//! epoch → cancel the old service (make-before-break). Task watchers hold the
//! epoch receiver and re-resume in-flight tasks on the fresh sink; task ids
//! are principal-scoped at the gateway, so continuity holds across rotations.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use rig_core::tool::{
    rmcp::{McpClientHandler, McpTaskResumer},
    server::ToolServerHandle,
};
use rmcp::{
    model::{ClientCapabilities, ClientInfo, Implementation},
    service::{RoleClient, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    delegate::KernelNotificationDelegate,
    elicitation::{ElicitationWaiters, ParkedElicitationHandler},
    ledger::KernelLedger,
    manifest::AgentManifest,
    wake::WakeBus,
};

/// Kernel surfaces wired into every gateway session (and re-wired on each
/// rotation): the wake bus behind the notification delegate and the parked
/// elicitation handler.
#[derive(Clone)]
pub struct KernelHandlers {
    pub bus: WakeBus,
    pub ledger: KernelLedger,
    pub waiters: ElicitationWaiters,
    pub elicitation_grace: Duration,
}

const CLIENT_ASSERTION_TYPE_JWT_BEARER: &str =
    "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";
const ASSERTION_TTL_SECONDS: u64 = 5 * 60;

#[derive(Debug, Serialize)]
struct ClientAssertionClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
}

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
}

/// One connected session: the running MCP service plus its resume surface.
struct Live {
    service: RunningService<RoleClient, McpClientHandler>,
    minted_at: Instant,
    token_ttl: Duration,
}

/// What task watchers subscribe to: bump = reconnect happened, re-resume.
/// `resumer` is `None` only for the pre-connection initial value, which no
/// watcher observes because `connect` rotates before returning.
#[derive(Clone)]
pub struct ConnectionEpoch {
    pub epoch: u64,
    pub resumer: Option<Arc<McpTaskResumer>>,
}

pub struct GatewayConnection {
    manifest: AgentManifest,
    tool_server_handle: ToolServerHandle,
    handlers: KernelHandlers,
    http: reqwest::Client,
    encoding_key: EncodingKey,
    live: Option<Live>,
    epoch: u64,
    epoch_tx: watch::Sender<ConnectionEpoch>,
}

impl GatewayConnection {
    /// Mint, connect, and publish the first epoch.
    pub async fn connect(
        manifest: AgentManifest,
        tool_server_handle: ToolServerHandle,
        handlers: KernelHandlers,
    ) -> Result<(Self, watch::Receiver<ConnectionEpoch>)> {
        let key_b64 = std::env::var(&manifest.gateway.private_key_env).with_context(|| {
            format!(
                "gateway private key env `{}` is not set",
                manifest.gateway.private_key_env
            )
        })?;
        let key_der = base64::engine::general_purpose::STANDARD
            .decode(key_b64.trim())
            .context("gateway private key must be base64 DER")?;
        let encoding_key = EncodingKey::from_rsa_der(&key_der);
        let http = reqwest::Client::new();

        let mut connection = Self {
            manifest,
            tool_server_handle,
            handlers,
            http,
            encoding_key,
            live: None,
            epoch: 0,
            epoch_tx: watch::channel(ConnectionEpoch {
                epoch: 0,
                resumer: None,
            })
            .0,
        };
        connection.rotate().await?;
        let epoch_rx = connection.epoch_tx.subscribe();
        Ok((connection, epoch_rx))
    }

    /// The current epoch's resumer, for arming watchers at boot.
    pub fn epoch(&self) -> ConnectionEpoch {
        self.epoch_tx.borrow().clone()
    }

    /// Rotate before the token enters its configured stale fraction.
    pub async fn ensure_fresh(&mut self) -> Result<()> {
        let stale = match &self.live {
            Some(live) => {
                live.minted_at.elapsed()
                    >= live
                        .token_ttl
                        .mul_f64(self.manifest.gateway.token_refresh_fraction)
            }
            None => true,
        };
        if stale { self.rotate().await } else { Ok(()) }
    }

    /// Make-before-break reconnect with a freshly minted token.
    pub async fn rotate(&mut self) -> Result<()> {
        let token = self.mint_token().await?;
        let transport = StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(self.manifest.mcp_url())
                .auth_header(token.access_token.clone()),
        );
        let handler = McpClientHandler::new(
            ClientInfo::new(
                ClientCapabilities::default(),
                Implementation::new("veoveo-agent", env!("CARGO_PKG_VERSION")),
            ),
            self.tool_server_handle.clone(),
        )
        .with_timeout(self.manifest.request_timeout())
        .with_notification_delegate(KernelNotificationDelegate::new(self.handlers.bus.clone()))
        .with_elicitation_handler(ParkedElicitationHandler::new(
            self.handlers.ledger.clone(),
            self.handlers.bus.clone(),
            self.handlers.waiters.clone(),
            self.handlers.elicitation_grace,
        ));
        let service = handler
            .connect(transport)
            .await
            .map_err(|err| anyhow::anyhow!("connecting to gateway MCP: {err}"))?;
        let resumer = Arc::new(service.service().task_resumer(service.peer().clone()));

        let previous = self.live.replace(Live {
            service,
            minted_at: Instant::now(),
            token_ttl: Duration::from_secs(token.expires_in),
        });
        self.epoch += 1;
        let epoch = self.epoch;
        self.epoch_tx.send_replace(ConnectionEpoch {
            epoch,
            resumer: Some(resumer),
        });
        tracing::info!(epoch, "gateway connection rotated");

        if let Some(previous) = previous {
            match previous.service.cancel().await {
                Ok(reason) => tracing::debug!(?reason, "previous gateway session closed"),
                Err(err) => tracing::warn!(%err, "previous gateway session join failed"),
            }
        }
        Ok(())
    }

    async fn mint_token(&self) -> Result<TokenEndpointResponse> {
        let gateway = &self.manifest.gateway;
        let now = chrono::Utc::now().timestamp();
        let now = u64::try_from(now).context("system clock is before the epoch")?;
        let claims = ClientAssertionClaims {
            iss: gateway.client_id.clone(),
            sub: gateway.client_id.clone(),
            aud: gateway.audience.clone(),
            exp: now + ASSERTION_TTL_SECONDS,
            nbf: now,
            iat: now,
            jti: uuid::Uuid::new_v4().to_string(),
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(gateway.private_key_kid.clone());
        let assertion = jsonwebtoken::encode(&header, &claims, &self.encoding_key)
            .context("signing client assertion")?;

        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer
            .append_pair("grant_type", "client_credentials")
            .append_pair("client_id", &gateway.client_id)
            .append_pair("scope", &gateway.scopes.join(" "))
            .append_pair("client_assertion_type", CLIENT_ASSERTION_TYPE_JWT_BEARER)
            .append_pair("client_assertion", &assertion)
            .append_pair("resource", &gateway.resource);
        let response = self
            .http
            .post(self.manifest.token_url())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(serializer.finish())
            .send()
            .await
            .context("posting to the gateway token endpoint")?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("token endpoint returned {status}: {body}");
        }
        let token: TokenEndpointResponse =
            serde_json::from_str(&body).context("parsing token endpoint response")?;
        if !token.token_type.eq_ignore_ascii_case("bearer") {
            bail!("token endpoint returned token_type `{}`", token.token_type);
        }
        if token.access_token.is_empty() || token.expires_in == 0 {
            bail!("token endpoint returned an unusable token");
        }
        Ok(token)
    }
}
