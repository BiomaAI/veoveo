//! Host validation shared by every hosted server.
//!
//! A request without a usable Host authority receives 400 Bad Request, as RFC 9112
//! section 3.2 requires. A request for an authority outside the installation's
//! allowed hosts receives 421 Misdirected Request.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{StatusCode, header::HOST},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{HostAuthority, ServerSlug, host_authority_is_allowed, parse_request_host_authority};

#[derive(Clone)]
pub(super) struct AllowedHosts {
    pub(super) hosts: Arc<Vec<String>>,
    pub(super) slug: ServerSlug,
}

pub(super) async fn validate_host(
    State(allowed): State<AllowedHosts>,
    request: Request,
    next: Next,
) -> Response {
    let Some(authority) = request_authority(&request) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if host_authority_is_allowed(&authority, &allowed.hosts) {
        return next.run(request).await;
    }
    tracing::warn!(
        server = allowed.slug.as_str(),
        host = authority.host(),
        port = authority.port(),
        "rejected request for untrusted host"
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
