//! Native MCP through the same authenticated gateway boundary as external clients.
use axum::http::{HeaderMap, HeaderValue, StatusCode, header::AUTHORIZATION};
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt,
    model::{ClientConfig, ProtocolVersion},
    service::{Peer, RoleClient, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use secrecy::{ExposeSecret, SecretString};
use std::num::NonZeroU16;
use std::{collections::HashMap, sync::Arc, time::Duration};
use veoveo_mcp_contract::{GatewayProfileId, PublicDeployment};

#[derive(Clone)]
pub struct NativeTransport {
    base: Arc<str>,
    host: HeaderValue,
    http: reqwest::Client,
    config: ClientConfig,
}

impl NativeTransport {
    pub fn new(
        port: NonZeroU16,
        deployment: &PublicDeployment,
        config: ClientConfig,
    ) -> anyhow::Result<Self> {
        let public = url::Url::parse(deployment.base_url())?;
        let host = &public[url::Position::BeforeHost..url::Position::AfterPort];
        Ok(Self {
            base: format!("http://127.0.0.1:{port}").into(),
            host: HeaderValue::from_str(host)?,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(80))
                .build()?,
            config,
        })
    }

    fn endpoint(&self, profile: &GatewayProfileId) -> anyhow::Result<url::Url> {
        let mut url = url::Url::parse(&self.base)?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("native gateway URL cannot carry path"))?
            .extend(["mcp", profile.as_str()]);
        Ok(url)
    }

    pub async fn connect(
        &self,
        profile: &GatewayProfileId,
        bearer: &SecretString,
    ) -> Result<NativeClient, StatusCode> {
        self.connect_with_progress(profile, bearer, None).await
    }

    pub async fn connect_with_progress(
        &self,
        profile: &GatewayProfileId,
        bearer: &SecretString,
        progress: Option<Arc<dyn ProgressObserver>>,
    ) -> Result<NativeClient, StatusCode> {
        let transport = StreamableHttpClientTransport::<reqwest::Client>::with_client(
            self.http.clone(),
            StreamableHttpClientTransportConfig::with_uri(
                self.endpoint(profile)
                    .map_err(|_| StatusCode::BAD_GATEWAY)?
                    .as_str()
                    .to_owned(),
            )
            .auth_header(bearer.expose_secret().to_owned())
            .custom_headers(HashMap::from([(reqwest::header::HOST, self.host.clone())])),
        );
        tokio::time::timeout(
            Duration::from_secs(8),
            Handler {
                progress,
                config: self.config.clone(),
            }
            .serve_with_lifecycle(
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

pub fn bearer(headers: &HeaderMap) -> Result<SecretString, StatusCode> {
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
struct Handler {
    progress: Option<Arc<dyn ProgressObserver>>,
    config: ClientConfig,
}
pub trait ProgressObserver: Send + Sync {
    fn observe(&self, params: rmcp::model::ProgressNotificationParam);
}

impl ClientHandler for Handler {
    async fn on_progress(
        &self,
        params: rmcp::model::ProgressNotificationParam,
        _: rmcp::service::NotificationContext<RoleClient>,
    ) {
        if let Some(progress) = &self.progress {
            progress.observe(params);
        }
    }
    fn get_info(&self) -> ClientConfig {
        self.config.clone()
    }
}

pub struct NativeClient {
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

mod catalog;
pub use catalog::{required_tools, tools};

pub fn mcp_error(error: rmcp::ServiceError) -> StatusCode {
    match error {
        rmcp::ServiceError::McpError(error)
            if error.code == rmcp::model::ErrorCode::INVALID_PARAMS =>
        {
            StatusCode::NOT_FOUND
        }
        rmcp::ServiceError::McpError(error)
            if error.code == rmcp::model::ErrorCode::INVALID_REQUEST =>
        {
            StatusCode::FORBIDDEN
        }
        _ => StatusCode::BAD_GATEWAY,
    }
}
