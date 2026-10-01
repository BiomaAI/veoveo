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

pub(super) fn router(
    state: Arc<super::AppState>,
    verifier: veoveo_mcp_contract::GatewayInternalTokenVerifier,
    allowed_hosts: AllowedHosts,
    mount_path: &str,
    cancellation: tokio_util::sync::CancellationToken,
) -> axum::Router {
    use super::{
        ReasonMcp, admin,
        internal_auth::{InternalMcpAuthState, authenticate_internal_mcp},
    };
    use axum::{Router, middleware, routing::get};
    use rmcp::transport::streamable_http_server::StreamableHttpService;
    use tower_http::trace::{DefaultMakeSpan, TraceLayer};
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(ReasonMcp::new(state.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(cancellation.child_token()),
    );
    let auth_state = InternalMcpAuthState { verifier };
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_internal_mcp,
        ));
    let admin_router = admin::router().layer(middleware::from_fn_with_state(
        auth_state,
        authenticate_internal_mcp,
    ));
    let service_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .with_state(state.clone())
        .nest("/admin", admin_router)
        .nest("/mcp", mcp_router);
    Router::new()
        .nest(mount_path, service_router)
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        )
}

async fn ready(State(state): State<Arc<super::AppState>>) -> StatusCode {
    if let Err(error) = state.recordings.readiness() {
        tracing::warn!("recording cache readiness failure: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    if let Err(error) = state.tasks.platform_store().healthcheck().await {
        tracing::warn!("reason readiness database failure: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    if let Err(error) = state.executor.readiness() {
        tracing::warn!("reason readiness runner failure: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    StatusCode::OK
}
