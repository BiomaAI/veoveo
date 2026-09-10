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
        if !matches!(request.url.scheme(), "http" | "https")
            || !request.url.username().is_empty()
            || request.url.password().is_some()
            || request.url.query().is_some()
            || request.url.fragment().is_some()
        {
            return Err(TransportError::Protocol);
        }
        let mut builder = self
            .0
            .get(request.url)
            .header(reqwest::header::AUTHORIZATION, request.authorization)
            .header(reqwest::header::ORIGIN, request.origin);
        if let Some(host) = request.host {
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
