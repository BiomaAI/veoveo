//! Request-scoped discovery recovery, before any domain request is dispatched.
use std::time::Duration;

use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::ProtocolVersion,
    service::{ClientInitializeError, RunningService},
    transport::{
        StreamableHttpClientTransport,
        streamable_http_client::{StreamableHttpClientTransportConfig, StreamableHttpError},
    },
};

use super::http_response::RequestError;
use super::upstream_authorized_http::{GatewayAuthorizedHttpClient, GatewayAuthorizedHttpError};
use crate::mcp_support::mcp_internal;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) async fn discover<S: ClientHandler + Clone>(
    handler: S,
    http: GatewayAuthorizedHttpClient,
    config: StreamableHttpClientTransportConfig,
) -> Result<RunningService<RoleClient, S>, RequestError> {
    for attempt in 0..2 {
        let transport = StreamableHttpClientTransport::with_client(http.clone(), config.clone());
        let result = tokio::time::timeout(
            DISCOVERY_TIMEOUT,
            handler.clone().serve_with_lifecycle(
                transport,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            ),
        )
        .await
        .map_err(|_| mcp_internal("upstream discovery exceeded ten seconds"))?;
        match result {
            Ok(running) => return Ok(running),
            Err(error) if attempt == 0 && recoverable(&error) => {
                // Discover is read-only. No tool, Task or domain request has been
                // dispatched, even when the caller intends a mutation afterward.
                tracing::warn!("retrying upstream discovery once after a transport failure");
            }
            Err(error) => {
                return Err(RequestError::from_initialize(error));
            }
        }
    }
    unreachable!("the final discovery attempt always returns")
}

fn recoverable(error: &ClientInitializeError) -> bool {
    let ClientInitializeError::TransportError { error, .. } = error else {
        return false;
    };
    match error
        .error
        .downcast_ref::<StreamableHttpError<GatewayAuthorizedHttpError>>()
    {
        Some(StreamableHttpError::Client(GatewayAuthorizedHttpError::Http(error))) => {
            error.is_connect() || error.is_timeout() || error.is_request() || error.is_body()
        }
        Some(
            StreamableHttpError::UnexpectedEndOfStream
            | StreamableHttpError::TransportChannelClosed,
        ) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
