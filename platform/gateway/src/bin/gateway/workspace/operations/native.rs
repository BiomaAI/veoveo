//! Native MCP through the same authenticated gateway boundary as external clients.
use axum::http::{HeaderMap, HeaderValue, StatusCode, header::AUTHORIZATION};
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt,
    model::{
        ClientCapabilities, ClientInfo, ElicitationCapability, Implementation, ProtocolVersion,
    },
    service::{Peer, RoleClient, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use secrecy::{ExposeSecret, SecretString};
use std::{collections::HashMap, sync::Arc, time::Duration};
use veoveo_mcp_contract::GatewayProfileId;

#[derive(Clone)]
pub(super) struct NativeTransport {
    base: Arc<str>,
    host: HeaderValue,
    http: reqwest::Client,
}

impl NativeTransport {
    pub fn new(port: u16, public_base: &str) -> anyhow::Result<Self> {
        let public = url::Url::parse(public_base)?;
        let host = &public[url::Position::BeforeHost..url::Position::AfterPort];
        Ok(Self {
            base: format!("http://127.0.0.1:{port}").into(),
            host: HeaderValue::from_str(host)?,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(80))
                .build()?,
        })
    }

    pub async fn connect(
        &self,
        profile: &GatewayProfileId,
        bearer: &SecretString,
    ) -> Result<NativeClient, StatusCode> {
        let transport = StreamableHttpClientTransport::<reqwest::Client>::with_client(
            self.http.clone(),
            StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp/{profile}", self.base))
                .auth_header(bearer.expose_secret().to_owned())
                .custom_headers(HashMap::from([(reqwest::header::HOST, self.host.clone())])),
        );
        tokio::time::timeout(
            Duration::from_secs(8),
            Handler.serve_with_lifecycle(
                transport,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            ),
        )
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map(|service| NativeClient { service })
        .map_err(|_| StatusCode::BAD_GATEWAY)
    }
}

pub(super) fn bearer(headers: &HeaderMap) -> Result<SecretString, StatusCode> {
    if headers.get_all(AUTHORIZATION).iter().count() != 1 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let (scheme, token) = header.split_once(' ').ok_or(StatusCode::UNAUTHORIZED)?;
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.bytes().any(|b| b.is_ascii_whitespace())
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(token.to_owned().into())
}

#[derive(Clone)]
struct Handler;
impl ClientHandler for Handler {
    fn get_info(&self) -> ClientInfo {
        let mut capabilities = ClientCapabilities::default();
        capabilities
            .extensions
            .get_or_insert_default()
            .entry(rmcp::model::TASKS_EXTENSION_ID.into())
            .or_default();
        capabilities.elicitation = Some(
            ElicitationCapability::new()
                .with_form(Default::default())
                .with_url(Default::default()),
        );
        ClientInfo::new(
            capabilities,
            Implementation::new("veoveo-workspace", env!("CARGO_PKG_VERSION")),
        )
    }
}

pub(super) struct NativeClient {
    service: RunningService<RoleClient, Handler>,
}
impl NativeClient {
    pub fn peer(&self) -> &Peer<RoleClient> {
        &self.service
    }
    pub async fn close(mut self) {
        let _ = self
            .service
            .close_with_timeout(Duration::from_secs(2))
            .await;
    }
}
