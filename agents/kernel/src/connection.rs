//! The agent's gateway session: identity, transport, and rotation.
//!
//! The agent is its own OAuth principal. Each session mints a gateway access
//! token via the client-credentials grant with a private-key JWT client
//! assertion (RFC 7523), attaches it as the streamable-HTTP `auth_header`,
//! and connects rig's `McpClientHandler` so gateway tools land on the shared
//! `ToolServerHandle`.
//!
//! rmcp fixes the auth header at transport construction, so token refresh is
//! connection rotation: mint → connect the replacement → restore declared
//! resource subscriptions → publish the new epoch → cancel the old service
//! (make-before-break). Task watchers hold the epoch receiver and re-resume
//! in-flight tasks on the fresh sink; task ids are principal-scoped at the
//! gateway, so continuity holds across rotations.

use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use rig::tool::{
    rmcp::{
        McpClientConfig, McpClientError, McpClientGuard, McpClientHandler, McpDeferredResolver,
        McpRequestHandle, McpRequestPreflight, McpResourceNotificationHandler,
    },
    server::ToolServerHandle,
};
use rig::wasm_compat::WasmBoxedFuture;
use rmcp::{
    model::{Implementation, ResourceUpdatedNotificationParam},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, watch};
use veoveo_agent_runtime::AgentRuntime;

use crate::{
    manifest::AgentManifest,
    wake::{self, WakeBus},
};

/// Kernel surfaces wired into every gateway session (and re-wired on each
/// rotation): the wake bus behind the notification delegate and the parked
/// input request handler.
#[derive(Clone)]
pub struct KernelHandlers {
    pub bus: WakeBus,
    pub runtime: AgentRuntime,
    pub input_grace: Duration,
}

#[derive(Clone)]
struct ResourceWakeHandler {
    declared: Arc<BTreeSet<String>>,
    bus: WakeBus,
}

impl McpResourceNotificationHandler for ResourceWakeHandler {
    fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
    ) -> WasmBoxedFuture<'_, ()> {
        Box::pin(async move {
            let Some(wake) = resource_update_wake(&self.declared, &params) else {
                tracing::error!(
                    uri = %params.uri,
                    "gateway delivered an update outside the acknowledged resource filter"
                );
                return;
            };
            if let Err(error) = self.bus.send(wake).await {
                tracing::error!(
                    %error,
                    uri = %params.uri,
                    "failed to persist resource update wake"
                );
            }
        })
    }
}

fn resource_update_wake(
    declared: &BTreeSet<String>,
    params: &ResourceUpdatedNotificationParam,
) -> Option<veoveo_agent_runtime::NewWake> {
    declared
        .contains(&params.uri)
        .then(|| wake::resource_updated(&params.uri))
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

#[derive(Deserialize)]
struct TokenEndpointResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
}

/// One connected session: the running MCP service plus its resume surface.
struct Live {
    guard: McpClientGuard,
    minted_at: Instant,
    token_ttl: Duration,
}

/// What task watchers subscribe to: bump = reconnect happened, re-resume.
/// `resumer` is `None` only for the pre-connection initial value, which no
/// watcher observes because `connect` rotates before returning.
#[derive(Clone)]
pub struct ConnectionEpoch {
    pub epoch: u64,
    pub resolver: Option<McpDeferredResolver>,
    pub request: Option<McpRequestHandle>,
}

struct GatewayConnectionInner {
    manifest: AgentManifest,
    tool_server_handle: ToolServerHandle,
    handlers: KernelHandlers,
    http: reqwest::Client,
    encoding_key: EncodingKey,
    live: Option<Live>,
    epoch: u64,
}

/// Serialized owner of the active gateway credential and MCP connection.
///
/// Every request preflight and explicit scheduler refresh shares the same
/// mutex. At most one caller can mint and publish a replacement epoch.
#[derive(Clone)]
pub struct GatewayConnection {
    inner: Arc<Mutex<GatewayConnectionInner>>,
    handlers: KernelHandlers,
    epoch_tx: watch::Sender<ConnectionEpoch>,
}

#[derive(Clone)]
struct RequestFreshness {
    inner: Weak<Mutex<GatewayConnectionInner>>,
    epoch_tx: watch::Sender<ConnectionEpoch>,
}

impl McpRequestPreflight for RequestFreshness {
    fn prepare(&self) -> WasmBoxedFuture<'_, Result<McpRequestHandle, McpClientError>> {
        Box::pin(async move {
            let Some(inner) = self.inner.upgrade() else {
                return Err(McpClientError::Unavailable);
            };
            let mut inner = inner.lock().await;
            if let Err(error) = inner.ensure_fresh(self.clone(), &self.epoch_tx).await {
                tracing::error!(%error, "gateway request preflight failed");
                return Err(McpClientError::Unavailable);
            }
            self.epoch_tx
                .borrow()
                .request
                .clone()
                .ok_or(McpClientError::Unavailable)
        })
    }
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
        let gateway_host = manifest
            .gateway_authority()?
            .parse()
            .context("gateway.url authority is not a valid HTTP Host header")?;
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(reqwest::header::HOST, gateway_host);
        let http = reqwest::Client::builder()
            .default_headers(default_headers)
            .build()
            .context("building gateway HTTP client")?;

        let (epoch_tx, epoch_rx) = watch::channel(ConnectionEpoch {
            epoch: 0,
            resolver: None,
            request: None,
        });
        let inner = Arc::new(Mutex::new(GatewayConnectionInner {
            manifest,
            tool_server_handle,
            handlers: handlers.clone(),
            http,
            encoding_key,
            live: None,
            epoch: 0,
        }));
        let connection = Self {
            inner,
            handlers,
            epoch_tx,
        };
        connection.rotate().await?;
        Ok((connection, epoch_rx))
    }

    /// The current epoch's resumer, for arming watchers at boot.
    pub fn epoch(&self) -> ConnectionEpoch {
        self.epoch_tx.borrow().clone()
    }

    pub fn handlers(&self) -> &KernelHandlers {
        &self.handlers
    }

    /// Rotate before the token enters its configured stale fraction.
    pub async fn ensure_fresh(&self) -> Result<()> {
        let preflight = self.request_freshness();
        self.inner
            .lock()
            .await
            .ensure_fresh(preflight, &self.epoch_tx)
            .await
    }

    /// Make-before-break reconnect with a freshly minted token.
    pub async fn rotate(&self) -> Result<()> {
        let preflight = self.request_freshness();
        self.inner
            .lock()
            .await
            .rotate(preflight, &self.epoch_tx)
            .await
    }

    fn request_freshness(&self) -> RequestFreshness {
        RequestFreshness {
            inner: Arc::downgrade(&self.inner),
            epoch_tx: self.epoch_tx.clone(),
        }
    }
}

impl GatewayConnectionInner {
    async fn ensure_fresh(
        &mut self,
        preflight: RequestFreshness,
        epoch_tx: &watch::Sender<ConnectionEpoch>,
    ) -> Result<()> {
        let stale = match &self.live {
            Some(live) => token_is_stale(
                live.minted_at,
                live.token_ttl,
                self.manifest.gateway.token_refresh_fraction,
                Instant::now(),
            ),
            None => true,
        };
        if stale {
            self.rotate(preflight, epoch_tx).await
        } else {
            Ok(())
        }
    }

    /// Make-before-break reconnect with a freshly minted token.
    async fn rotate(
        &mut self,
        preflight: RequestFreshness,
        epoch_tx: &watch::Sender<ConnectionEpoch>,
    ) -> Result<()> {
        let token = self.mint_token().await?;
        let mut transport_headers = HashMap::new();
        transport_headers.insert(
            reqwest::header::HOST,
            self.manifest
                .gateway_authority()?
                .parse()
                .context("gateway.url authority is not a valid HTTP Host header")?,
        );
        let transport = StreamableHttpClientTransport::with_client(
            self.http.clone(),
            StreamableHttpClientTransportConfig::with_uri(self.manifest.mcp_url())
                .auth_header(token.access_token.clone())
                .custom_headers(transport_headers),
        );
        let config = McpClientConfig::new(Implementation::new(
            "veoveo-agent",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_deferred_backend_id("mcp:veoveo-gateway");
        let declared_resources = Arc::new(
            self.manifest
                .resource_subscriptions
                .iter()
                .map(|subscription| subscription.uri.clone())
                .collect::<BTreeSet<_>>(),
        );
        let handler = McpClientHandler::new(config, self.tool_server_handle.clone())
            .with_timeout(self.manifest.request_timeout())
            .with_request_preflight(preflight);
        let handler = if declared_resources.is_empty() {
            handler
        } else {
            handler.with_resource_subscriptions(
                declared_resources.iter().cloned(),
                ResourceWakeHandler {
                    declared: declared_resources.clone(),
                    bus: self.handlers.bus.clone(),
                },
            )
        };
        let guard = handler
            .connect(transport)
            .await
            .map_err(|err| anyhow::anyhow!("connecting to gateway MCP: {err}"))?;
        let subscription_count = declared_resources.len();
        let resolver = guard.deferred_resolver();
        let request = guard.request_handle();

        let previous = self.live.replace(Live {
            guard,
            minted_at: Instant::now(),
            token_ttl: Duration::from_secs(token.expires_in),
        });
        self.epoch += 1;
        let epoch = self.epoch;
        epoch_tx.send_replace(ConnectionEpoch {
            epoch,
            resolver: Some(resolver),
            request: Some(request),
        });
        tracing::info!(epoch, subscription_count, "gateway connection rotated");

        if let Some(previous) = previous {
            match previous.guard.cancel().await {
                Ok(()) => tracing::debug!("previous gateway connection closed"),
                Err(err) => tracing::warn!(%err, "previous gateway connection close failed"),
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
            jti: uuid::Uuid::now_v7().to_string(),
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(gateway.private_key_kid.clone());
        let assertion = jsonwebtoken::encode(&header, &claims, &self.encoding_key)
            .context("signing client assertion")?;

        let body = {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            serializer
                .append_pair("grant_type", "client_credentials")
                .append_pair("client_id", &gateway.client_id)
                .append_pair("work_context", &gateway.work_context)
                .append_pair("scope", &gateway.scopes.join(" "))
                .append_pair("client_assertion_type", CLIENT_ASSERTION_TYPE_JWT_BEARER)
                .append_pair("client_assertion", &assertion)
                .append_pair("resource", &gateway.resource);
            serializer.finish()
        };
        let response = self
            .http
            .post(self.manifest.token_url())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
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

fn token_is_stale(
    minted_at: Instant,
    token_ttl: Duration,
    refresh_fraction: f64,
    now: Instant,
) -> bool {
    now.saturating_duration_since(minted_at) >= token_ttl.mul_f64(refresh_fraction)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rmcp::model::ResourceUpdatedNotificationParam;
    use veoveo_platform_store::WakeKind;

    use super::{resource_update_wake, token_is_stale};

    #[test]
    fn resource_updates_create_wakes_only_for_declared_uris() {
        let declared =
            BTreeSet::from(["memo://insights".to_owned(), "memo://knowledge".to_owned()]);

        let wake = resource_update_wake(
            &declared,
            &ResourceUpdatedNotificationParam::new("memo://insights"),
        )
        .expect("declared resource wake");
        assert_eq!(wake.kind, WakeKind::ResourceChanged);
        assert_eq!(wake.dedupe_key.as_deref(), Some("resource:memo://insights"));
        assert_eq!(
            wake.payload
                .as_map()
                .get("uri")
                .and_then(|uri| uri.as_str()),
            Some("memo://insights")
        );

        assert!(
            resource_update_wake(
                &declared,
                &ResourceUpdatedNotificationParam::new("memo://undeclared"),
            )
            .is_none()
        );
    }

    #[test]
    fn request_freshness_uses_the_configured_token_fraction() {
        let now = std::time::Instant::now();
        let ttl = std::time::Duration::from_secs(100);

        assert!(!token_is_stale(
            now - std::time::Duration::from_secs(79),
            ttl,
            0.8,
            now,
        ));
        assert!(token_is_stale(
            now - std::time::Duration::from_secs(80),
            ttl,
            0.8,
            now,
        ));
    }
}
