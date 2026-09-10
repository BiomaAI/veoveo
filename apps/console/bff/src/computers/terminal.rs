use super::{fault, origin};
use crate::{AppState, api};
use axum::{
    extract::{OriginalUri, Path, State, ws::WebSocketUpgrade},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::Response,
};
use uuid::Uuid;
use veoveo_computers_transport::{MAX_MESSAGE_BYTES, UpstreamRequest, relay};

pub(super) async fn upgrade(
    State(state): State<AppState>,
    id: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let Ok(Path(id)) = id else {
        return fault(StatusCode::BAD_REQUEST);
    };
    if id.is_nil() || uri.query().is_some() {
        return fault(StatusCode::BAD_REQUEST);
    }
    let origin = match origin(&state, &headers) {
        Ok(origin) => origin,
        Err(status) => return fault(status),
    };
    let session = match api::upstream_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let response_headers = match api::response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return fault(status),
    };
    let mut response = match admitted(state, id, origin, &session.session.access_token, ws).await {
        Ok(response) | Err(response) => response,
    };
    // WebSocket upgrades may carry the rotated cookie; the ticket is bound to its family.
    response.headers_mut().extend(response_headers);
    response
}

async fn admitted(
    state: AppState,
    id: Uuid,
    origin: HeaderValue,
    token: &str,
    ws: WebSocketUpgrade,
) -> Result<Response, Response> {
    let slot = state
        .computers
        .slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| fault(StatusCode::SERVICE_UNAVAILABLE))?;
    let mut authorization = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| fault(StatusCode::UNAUTHORIZED))?;
    authorization.set_sensitive(true);
    let host = HeaderValue::from_str(&state.config.gateway_host())
        .map_err(|_| fault(StatusCode::SERVICE_UNAVAILABLE))?;
    let upstream = tokio::select! {
        biased;
        _ = state.computers.stop.cancelled() => return Err(fault(StatusCode::SERVICE_UNAVAILABLE)),
        result = state.computers.client.connect(UpstreamRequest { url: state.config.computers_url(&format!("/{id}/terminal")), authorization, host: Some(host), origin }) => result.map_err(|_| fault(StatusCode::SERVICE_UNAVAILABLE))?,
    };
    Ok(ws
        .max_frame_size(MAX_MESSAGE_BYTES)
        .max_message_size(MAX_MESSAGE_BYTES)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_MESSAGE_BYTES * 2)
        .on_upgrade(move |socket| async move {
            let _slot = slot;
            let _ = relay(socket, upstream, id, state.computers.stop).await;
        }))
}
