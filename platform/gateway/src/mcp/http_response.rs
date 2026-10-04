//! Preserve a final upstream HTTP rejection through the MCP handler boundary.
use std::sync::{Arc, OnceLock};

use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use rmcp::{
    model::ErrorData as McpError,
    service::{ClientInitializeError, RequestContext, RoleServer},
    transport::{DynamicTransportError, streamable_http_client::StreamableHttpError},
};

use super::upstream_authorized_http::GatewayAuthorizedHttpError;
use crate::mcp_support::{mcp_internal, mcp_invalid_request};

#[derive(Debug, thiserror::Error)]
pub(super) enum RequestError {
    #[error(transparent)]
    Protocol(#[from] McpError),
    #[error("{error}")]
    Http { status: StatusCode, error: McpError },
}

impl RequestError {
    pub(super) fn protocol(&self) -> &McpError {
        match self {
            Self::Protocol(error) | Self::Http { error, .. } => error,
        }
    }

    /// Discovery can isolate one source failure and return a successful catalog.
    /// Discarding that source's HTTP status must not affect the request response.
    pub(super) fn into_protocol(self) -> McpError {
        match self {
            Self::Protocol(error) | Self::Http { error, .. } => error,
        }
    }

    pub(super) fn from_initialize(error: ClientInitializeError) -> Self {
        if let ClientInitializeError::TransportError { error, .. } = &error
            && let Some(rejection) = Self::from_transport(error)
        {
            return rejection;
        }
        mcp_internal(format!("failed to discover upstream MCP: {error}")).into()
    }

    fn from_transport(error: &DynamicTransportError) -> Option<Self> {
        match error
            .error
            .downcast_ref::<StreamableHttpError<GatewayAuthorizedHttpError>>()
        {
            Some(StreamableHttpError::HttpResponse { status, body }) => Some(Self::Http {
                status: if status.is_client_error() || status.is_server_error() {
                    *status
                } else {
                    StatusCode::BAD_GATEWAY
                },
                error: mcp_internal(format!("upstream HTTP request rejected: {status}: {body}")),
            }),
            _ => None,
        }
    }
}

impl From<rmcp::ServiceError> for RequestError {
    fn from(error: rmcp::ServiceError) -> Self {
        match error {
            rmcp::ServiceError::McpError(error) => Self::Protocol(error),
            rmcp::ServiceError::TransportSend(ref transport) => Self::from_transport(transport)
                .unwrap_or_else(|| {
                    mcp_internal(format!("upstream MCP request failed: {error}")).into()
                }),
            error => mcp_internal(format!("upstream MCP request failed: {error}")).into(),
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct HttpResponseContext(Arc<OnceLock<StatusCode>>);

impl HttpResponseContext {
    pub(super) fn from_request(context: &RequestContext<RoleServer>) -> Result<Self, McpError> {
        context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<Self>())
            .cloned()
            .ok_or_else(|| mcp_invalid_request("HTTP response context missing"))
    }

    pub(super) fn finish<T>(&self, result: Result<T, RequestError>) -> Result<T, McpError> {
        result.map_err(|error| {
            if let RequestError::Http { status, .. } = &error {
                let _ = self.0.set(*status);
            }
            error.into_protocol()
        })
    }
}

/// The hosted profile serializes ordinary MCP responses as JSON. Set the status
/// only after its handler returns a final rejection, before HTTP headers are sent.
pub async fn preserve_upstream_http_rejection(mut request: Request, next: Next) -> Response {
    let context = HttpResponseContext::default();
    request.extensions_mut().insert(context.clone());
    let mut response = next.run(request).await;
    if response.status() == StatusCode::OK
        && let Some(status) = context.0.get()
    {
        *response.status_mut() = *status;
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::Extension, middleware, routing::get};
    use tower::ServiceExt;

    fn transport_rejection(status: StatusCode) -> rmcp::ServiceError {
        rmcp::ServiceError::TransportSend(DynamicTransportError::from_parts(
            "fixture",
            std::any::TypeId::of::<()>(),
            Box::new(
                StreamableHttpError::<GatewayAuthorizedHttpError>::HttpResponse {
                    status,
                    body: "source rejection".to_owned(),
                },
            ),
        ))
    }

    #[test]
    fn typed_http_rejection_does_not_retry_but_connection_loss_does() {
        let rejection = transport_rejection(StatusCode::PAYLOAD_TOO_LARGE);
        assert!(!super::super::recoverable_upstream_connection_error(
            &rejection
        ));
        assert!(super::super::recoverable_upstream_connection_error(
            &rmcp::ServiceError::TransportClosed
        ));
        assert!(matches!(
            RequestError::from(rejection),
            RequestError::Http {
                status: StatusCode::PAYLOAD_TOO_LARGE,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn isolated_discovery_failure_and_protocol_errors_keep_http_success() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let router = Router::new()
                .route(
                    "/discovery",
                    get(
                        |Extension(context): Extension<HttpResponseContext>| async move {
                            let rejection = RequestError::from(transport_rejection(
                                StatusCode::SERVICE_UNAVAILABLE,
                            ));
                            let (prompts, _, errors) =
                                super::super::discovery::isolate_discovery_failures(
                                    veoveo_gateway_contract::GatewayDiscoverySurface::Prompts,
                                    vec![
                                        ("healthy".parse().unwrap(), Ok(vec!["available-prompt"])),
                                        (
                                            "offline".parse().unwrap(),
                                            Err(rejection.into_protocol()),
                                        ),
                                    ],
                                );
                            assert_eq!(errors.len(), 1);
                            axum::Json(context.finish(Ok(prompts)).unwrap())
                        },
                    ),
                )
                .route(
                    "/protocol",
                    get(
                        |Extension(context): Extension<HttpResponseContext>| async move {
                            // A domain error can contain HTTP-looking text or data. Neither is transport authority.
                            let error = McpError::invalid_params(
                                "HTTP 413",
                                Some(serde_json::json!({"status": 413})),
                            );
                            axum::Json(context.finish::<()>(Err(error.into())).unwrap_err())
                        },
                    ),
                )
                .layer(middleware::from_fn(preserve_upstream_http_rejection));
            let (catalog, protocol) = tokio::join!(
                router.clone().oneshot(
                    Request::builder()
                        .uri("/discovery")
                        .body(axum::body::Body::empty())
                        .unwrap()
                ),
                router.oneshot(
                    Request::builder()
                        .uri("/protocol")
                        .body(axum::body::Body::empty())
                        .unwrap()
                )
            );
            let catalog = catalog.unwrap();
            let protocol = protocol.unwrap();
            assert_eq!(catalog.status(), StatusCode::OK);
            assert_eq!(protocol.status(), StatusCode::OK);
            let catalog = axum::body::to_bytes(catalog.into_body(), 4096)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<Vec<String>>(&catalog).unwrap(),
                vec!["available-prompt"]
            );
            let protocol = axum::body::to_bytes(protocol.into_body(), 4096)
                .await
                .unwrap();
            let protocol: McpError = serde_json::from_slice(&protocol).unwrap();
            assert_eq!(protocol.code, rmcp::model::ErrorCode::INVALID_PARAMS);
            assert_eq!(protocol.data, Some(serde_json::json!({"status": 413})));
        })
        .await
        .expect("HTTP response isolation regression exceeded five seconds");
    }
}
