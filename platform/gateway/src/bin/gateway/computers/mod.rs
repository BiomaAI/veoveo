//! Core Computer HTTP admission; the hosted domain owns lifecycle and renewal.
mod authority;
mod cli;
mod control;
mod maintenance;
mod routes;
mod terminal;

use axum::{
    Json, Router,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use veoveo_computers_contract::{ApiError, ErrorCode};
use veoveo_mcp_contract::GatewayInternalTokenIssuer;
use veoveo_mcp_gateway::{GatewayCatalogHandle, GatewayState, GatewayUpstreamHttpClientPool};

#[derive(Clone)]
pub(crate) struct ComputersState {
    pub catalog: GatewayCatalogHandle,
    pub gateway_state: GatewayState,
    pub issuer: GatewayInternalTokenIssuer,
    pub upstream: GatewayUpstreamHttpClientPool,
    pub origin: HeaderValue,
    pub slots: Arc<Semaphore>,
    pub stop: CancellationToken,
}

pub(crate) fn router(state: ComputersState) -> Router {
    Router::new()
        .route(
            "/computers/{profile}",
            get(control::proxy).post(control::proxy),
        )
        .route("/computers/{profile}/{id}", get(control::proxy))
        .route("/computers/{profile}/{id}/access", get(control::proxy))
        .route(
            "/computers/{profile}/{id}/automation",
            get(control::proxy).post(control::proxy),
        )
        .route(
            "/computers/{profile}/{id}/automation/{grant_id}",
            get(control::proxy),
        )
        .route(
            "/computers/{profile}/{id}/automation/{grant_id}/revoke",
            post(control::proxy),
        )
        .route(
            "/computers/{profile}/{id}/cli-pairings",
            post(control::proxy),
        )
        .route(
            "/computers/{profile}/{id}/cli-pairings/{pairing_id}/confirm",
            post(control::proxy),
        )
        .route(
            "/computers/{profile}/{id}/access/{grant_id}/revoke",
            post(control::proxy),
        )
        .route(
            "/computers/{profile}/{id}/operations/{operation_id}",
            get(control::proxy),
        )
        .route("/computers/{profile}/{id}/start", post(control::proxy))
        .route("/computers/{profile}/{id}/maintenance", get(control::proxy))
        .route(
            "/computers/{profile}/{id}/maintenance/{operation_id}",
            get(control::proxy),
        )
        .route(
            "/computers/{profile}/{id}/update-template",
            post(control::proxy),
        )
        .route(
            "/computers/{profile}/{id}/maintenance/{operation_id}/resume",
            post(control::proxy),
        )
        .route("/computers/{profile}/{id}/stop", post(control::proxy))
        .route(
            "/computers/{profile}/{id}/terminal-ticket",
            post(control::proxy),
        )
        .route("/computers/{profile}/{id}/terminal", get(terminal::upgrade))
        .with_state(state)
}
pub(crate) fn cli_router(state: ComputersState) -> Router {
    cli::router(state)
}

#[derive(Debug)]
struct Fault(StatusCode, ErrorCode);
impl Fault {
    fn invalid() -> Self {
        Self(StatusCode::BAD_REQUEST, ErrorCode::InvalidInput)
    }
    fn denied() -> Self {
        Self(StatusCode::FORBIDDEN, ErrorCode::Forbidden)
    }
    fn unavailable() -> Self {
        Self(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable)
    }
    fn missing() -> Self {
        Self(StatusCode::NOT_FOUND, ErrorCode::NotFound)
    }
}
impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        let message = match self.1 {
            ErrorCode::InvalidInput => "The Computer request is invalid.",
            ErrorCode::Forbidden => "Current access does not allow this Computer action.",
            ErrorCode::NotFound => "This Computer is unavailable in the current Work Context.",
            _ => "Computers is temporarily unavailable. Try again with the same request.",
        };
        (
            self.0,
            [(header::CACHE_CONTROL, "no-store")],
            Json(ApiError {
                code: self.1,
                message: message.into(),
            }),
        )
            .into_response()
    }
}

fn exact_origin(
    headers: &axum::http::HeaderMap,
    expected: &HeaderValue,
) -> Result<HeaderValue, Fault> {
    let mut values = headers.get_all(header::ORIGIN).iter();
    match (values.next(), values.next()) {
        (Some(value), None) if value == expected => Ok(value.clone()),
        _ => Err(Fault::denied()),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_admission;
