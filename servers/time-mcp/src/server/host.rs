use axum::{
    extract::{Request, State},
    http::{StatusCode, header::HOST},
    middleware::Next,
    response::IntoResponse,
};
use std::sync::Arc;
use veoveo_mcp_contract::{host_authority_is_allowed, parse_request_host_authority};

pub(super) type AllowedHosts = Arc<Vec<String>>;
pub(super) async fn validate_host(
    State(allowed_hosts): State<AllowedHosts>,
    request: Request,
    next: Next,
) -> axum::response::Response {
    let authority = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_request_host_authority)
        .or_else(|| {
            request
                .uri()
                .authority()
                .and_then(|authority| parse_request_host_authority(authority.as_str()))
        });
    if authority
        .as_ref()
        .is_some_and(|authority| host_authority_is_allowed(authority, &allowed_hosts))
    {
        next.run(request).await
    } else {
        StatusCode::MISDIRECTED_REQUEST.into_response()
    }
}
