//! Stock client edge headers become a narrow credential; Console cookies are unused.
use super::fault;
use crate::AppState;
use axum::{
    extract::{Path, RawQuery, State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use uuid::Uuid;
use veoveo_computers_transport::{
    CliCredentialFraming, CliRelayMode, CliUpstreamRequest, MAX_MESSAGE_BYTES, cli_authorization,
    relay_cli,
};

pub(super) async fn root(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    upgrade(state, None, query, headers, ws).await
}
pub(super) async fn scoped(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    upgrade(state, Some(id), query, headers, ws).await
}
async fn upgrade(
    state: AppState,
    computer: Option<Uuid>,
    query: Option<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if computer.is_some_and(|id| id.is_nil()) {
        return fault(StatusCode::BAD_REQUEST);
    }
    let Ok(authorization) =
        cli_authorization(&headers, CliCredentialFraming::Stock, query.as_deref())
    else {
        return fault(StatusCode::FORBIDDEN);
    };
    let Ok(slot) = state.computers.slots.clone().try_acquire_owned() else {
        return fault(StatusCode::SERVICE_UNAVAILABLE);
    };
    let suffix = computer.map(|id| format!("/{id}")).unwrap_or_default();
    let url = state
        .config
        .computers_url(&format!("/cli{suffix}/_ws_tunnel"));
    let Ok(host) = state.config.gateway_host().parse() else {
        return fault(StatusCode::SERVICE_UNAVAILABLE);
    };
    let upstream = tokio::select! {
        biased;
        _ = state.computers.stop.cancelled() => return fault(StatusCode::SERVICE_UNAVAILABLE),
        result = state.computers.client.connect_cli(CliUpstreamRequest { url, authorization, host: Some(host) }) => result,
    };
    let Ok(upstream) = upstream else {
        return fault(StatusCode::FORBIDDEN);
    };
    let mut response = ws
        .max_frame_size(MAX_MESSAGE_BYTES)
        .max_message_size(MAX_MESSAGE_BYTES)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_MESSAGE_BYTES * 2)
        .on_upgrade(move |socket| async move {
            let _slot = slot;
            let _ = relay_cli(
                socket,
                upstream,
                CliRelayMode::PublicClient,
                state.computers.stop,
            )
            .await;
        });
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    response
}
