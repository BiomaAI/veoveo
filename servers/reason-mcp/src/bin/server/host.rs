use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{StatusCode, header::HOST},
    middleware::Next,
    response::IntoResponse,
};
use veoveo_mcp_contract::{HostAuthority, host_authority_is_allowed, parse_request_host_authority};

pub(super) type AllowedHosts = Arc<Vec<String>>;

pub(super) async fn validate_host(
    State(allowed_hosts): State<AllowedHosts>,
    request: Request,
    next: Next,
) -> axum::response::Response {
    let Some(authority) = request_authority(&request) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if host_authority_is_allowed(&authority, &allowed_hosts) {
        return next.run(request).await;
    }
    tracing::warn!(
        host = authority.host(),
        port = authority.port(),
        "rejected reason request for untrusted host"
    );
    StatusCode::MISDIRECTED_REQUEST.into_response()
}

fn request_authority(request: &Request) -> Option<HostAuthority> {
    if let Some(header) = request.headers().get(HOST) {
        return header.to_str().ok().and_then(parse_request_host_authority);
    }
    request
        .uri()
        .authority()
        .and_then(|authority| parse_request_host_authority(authority.as_str()))
}
