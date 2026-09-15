//! Native Computers use the Console session and the gateway's current domain policy.
mod cli;
mod control;
mod events;
mod pairing;
mod terminal;

use crate::{AppState, browser::BrowserApp, outbound_http::OutboundTrust};
use axum::{
    Json, Router,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use veoveo_computers_contract::{ApiError, ErrorCode};

#[derive(Clone)]
pub(crate) struct Transport {
    client: veoveo_computers_transport::Client,
    slots: Arc<Semaphore>,
    pub(crate) stop: CancellationToken,
}
impl Transport {
    pub(crate) fn new(trust: &OutboundTrust) -> anyhow::Result<Self> {
        Ok(Self {
            client: veoveo_computers_transport::Client::new(trust.client_builder())?,
            slots: Arc::new(Semaphore::new(128)),
            stop: CancellationToken::new(),
        })
    }
}
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/_ws_tunnel", get(cli::root))
        .route("/console/computers/{id}/_ws_tunnel", get(cli::scoped))
        .route("/console/computers/{id}/auth/connect", get(pairing::entry))
        .merge(control_router(BrowserApp::Console))
}

pub(crate) fn control_router(app: BrowserApp) -> Router<AppState> {
    let root = app.api_root();
    Router::new()
        .route(
            &format!("{root}/computers/events"),
            post(events::events).layer(axum::extract::DefaultBodyLimit::max(1024)),
        )
        .route(
            &format!("{root}/computers"),
            get(control::proxy).post(control::proxy),
        )
        .route(&format!("{root}/computers/{{id}}"), get(control::proxy))
        .route(
            &format!("{root}/computers/{{id}}/files"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/files/{{operation_id}}"),
            get(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/files/{{operation_id}}/cancel"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/access"),
            get(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/automation"),
            get(control::proxy).post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/automation/{{grant_id}}"),
            get(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/automation/{{grant_id}}/revoke"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/cli-pairings"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/cli-pairings/{{pairing_id}}/confirm"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/access/{{grant_id}}/revoke"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/operations/{{operation_id}}"),
            get(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/start"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/maintenance"),
            get(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/maintenance/{{operation_id}}"),
            get(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/update-template"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/maintenance/{{operation_id}}/resume"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/stop"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/terminal-ticket"),
            post(control::proxy),
        )
        .route(
            &format!("{root}/computers/{{id}}/terminal"),
            get(terminal::upgrade),
        )
}
fn origin(state: &AppState, headers: &HeaderMap) -> Result<HeaderValue, StatusCode> {
    let expected = state.config.public_origin();
    let mut values = headers.get_all(header::ORIGIN).iter();
    match (values.next(), values.next()) {
        (Some(value), None) if value.as_bytes() == expected.as_bytes() => Ok(value.clone()),
        _ => Err(StatusCode::FORBIDDEN),
    }
}
fn fault(status: StatusCode) -> Response {
    let (code, message) = match status {
        StatusCode::FORBIDDEN => (
            ErrorCode::Forbidden,
            "Current access does not allow this Computer connection.",
        ),
        StatusCode::BAD_REQUEST | StatusCode::PAYLOAD_TOO_LARGE => {
            (ErrorCode::InvalidInput, "The Computer request is invalid.")
        }
        _ => (
            ErrorCode::Unavailable,
            "Computers is temporarily unavailable. Try again with the same request.",
        ),
    };
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(ApiError {
            code,
            message: message.into(),
        }),
    )
        .into_response()
}

pub(crate) async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests;
