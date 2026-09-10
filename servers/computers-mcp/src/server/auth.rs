use axum::{
    extract::{Request, State},
    http::{
        StatusCode,
        header::{AUTHORIZATION, HOST},
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use veoveo_mcp_contract::{
    GatewayInternalTokenVerifier, host_authority_is_allowed, parse_request_host_authority,
};

/// Bound request admission and database work. A subscription's response body has
/// its own renewable authority deadline after the HTTP response is established.
pub async fn deadline(request: Request, next: Next) -> Response {
    tokio::time::timeout(std::time::Duration::from_secs(30), next.run(request))
        .await
        .unwrap_or_else(|_| {
            (
                StatusCode::GATEWAY_TIMEOUT,
                "Computer request timed out; reuse its requestId to resolve the outcome",
            )
                .into_response()
        })
}

pub async fn internal(
    State(verifier): State<GatewayInternalTokenVerifier>,
    mut request: Request,
    next: Next,
) -> Response {
    let identity = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(kind, token)| {
            kind.eq_ignore_ascii_case("bearer")
                && !token.is_empty()
                && !token.chars().any(char::is_whitespace)
        })
        .and_then(|(_, token)| verifier.verify(token).ok());
    let Some(identity) = identity else {
        return (StatusCode::UNAUTHORIZED, "gateway authorization required").into_response();
    };
    request.extensions_mut().insert(identity);
    next.run(request).await
}
pub async fn host(State(hosts): State<Arc<Vec<String>>>, request: Request, next: Next) -> Response {
    let authority = request
        .headers()
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_request_host_authority)
        .or_else(|| {
            request
                .uri()
                .authority()
                .and_then(|v| parse_request_host_authority(v.as_str()))
        });
    if !authority.is_some_and(|a| host_authority_is_allowed(&a, &hosts)) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    next.run(request).await
}
