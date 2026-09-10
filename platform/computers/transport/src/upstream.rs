use crate::{MAX_MESSAGE_BYTES, Result, TransportError};
use reqwest_websocket::Upgrade;
use std::time::Duration;
use tungstenite::protocol::WebSocketConfig;

/// No Debug implementation: an upgraded stream may retain credentialed request context.
pub struct Upstream(pub(crate) reqwest_websocket::WebSocket);

#[derive(Clone)]
pub struct Client(reqwest::Client);

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ComputerTransportClient")
    }
}
/// The owning application supplies its admitted destination and current authority.
/// No Debug implementation can expose the authorization header.
pub struct UpstreamRequest {
    pub url: reqwest::Url,
    pub authorization: reqwest::header::HeaderValue,
    pub host: Option<reqwest::header::HeaderValue>,
    pub origin: reqwest::header::HeaderValue,
}

/// A nonbrowser CLI grant, selected by the owning fixed-route adapter. No browser
/// Origin or cookie authority may be inferred from this request.
pub struct CliUpstreamRequest {
    pub url: reqwest::Url,
    pub authorization: reqwest::header::HeaderValue,
    pub host: Option<reqwest::header::HeaderValue>,
}

impl Client {
    pub fn new(builder: reqwest::ClientBuilder) -> Result<Self> {
        builder
            .http1_only()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map(Self)
            .map_err(|_| TransportError::Interrupted)
    }

    pub async fn connect(&self, request: UpstreamRequest) -> Result<Upstream> {
        self.connect_admitted(
            request.url,
            request.authorization,
            request.host,
            Some(request.origin),
        )
        .await
    }

    pub async fn connect_cli(&self, request: CliUpstreamRequest) -> Result<Upstream> {
        self.connect_admitted(request.url, request.authorization, request.host, None)
            .await
    }

    async fn connect_admitted(
        &self,
        url: reqwest::Url,
        authorization: reqwest::header::HeaderValue,
        host: Option<reqwest::header::HeaderValue>,
        origin: Option<reqwest::header::HeaderValue>,
    ) -> Result<Upstream> {
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(TransportError::Protocol);
        }
        let mut builder = self
            .0
            .get(url)
            .header(reqwest::header::AUTHORIZATION, authorization);
        if let Some(origin) = origin {
            builder = builder.header(reqwest::header::ORIGIN, origin);
        }
        if let Some(host) = host {
            builder = builder.header(reqwest::header::HOST, host);
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            let config = WebSocketConfig::default()
                .write_buffer_size(0)
                .max_write_buffer_size(MAX_MESSAGE_BYTES * 2)
                .max_message_size(Some(MAX_MESSAGE_BYTES))
                .max_frame_size(Some(MAX_MESSAGE_BYTES));
            let response = builder
                .version(reqwest::Version::HTTP_11)
                .upgrade()
                .web_socket_config(config)
                .send()
                .await
                .map_err(|_| TransportError::Interrupted)?;
            if response
                .headers()
                .contains_key(reqwest::header::SEC_WEBSOCKET_EXTENSIONS)
            {
                return Err(TransportError::Protocol);
            }
            response
                .into_websocket()
                .await
                .map(Upstream)
                .map_err(|_| TransportError::Interrupted)
        })
        .await
        .map_err(|_| TransportError::Interrupted)?
    }
}
