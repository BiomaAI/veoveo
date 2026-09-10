use super::{
    ComputersState, Fault,
    authority::authorize,
    exact_origin,
    routes::{Operation, Route},
};
use axum::{
    extract::{Extension, OriginalUri, Path, State, ws::WebSocketUpgrade},
    http::HeaderMap,
    response::Response,
};
use veoveo_computers_transport::{MAX_MESSAGE_BYTES, UpstreamRequest, relay};
use veoveo_mcp_gateway::AuthenticatedSubject;

pub(super) async fn upgrade(
    State(state): State<ComputersState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    route: Result<Path<Route>, axum::extract::rejection::PathRejection>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, Fault> {
    let Path(route) = route.map_err(|_| Fault::invalid())?;
    let id = route
        .id
        .filter(|id| !id.is_nil())
        .ok_or_else(Fault::invalid)?;
    if uri.query().is_some() {
        return Err(Fault::invalid());
    }
    let origin = exact_origin(&headers, &state.origin)?;
    let admitted = authorize(&state, &route, Operation::Terminal(id), subject).await?;
    let slot = state
        .slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Fault::unavailable())?;
    let upstream = tokio::select! {
        biased;
        _ = state.stop.cancelled() => return Err(Fault::unavailable()),
        result = async {
            let client = state.upstream.websocket_client(&admitted.catalog, &admitted.manifest).await.map_err(|_| Fault::unavailable())?;
            client.connect(UpstreamRequest { url: admitted.url, authorization: admitted.authorization, host: None, origin }).await.map_err(|_| Fault::unavailable())
        } => result?,
    };
    Ok(ws
        .max_frame_size(MAX_MESSAGE_BYTES)
        .max_message_size(MAX_MESSAGE_BYTES)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_MESSAGE_BYTES * 2)
        .on_upgrade(move |socket| async move {
            let _slot = slot;
            // The service owns renewable authority. Relay errors contain no payloads.
            let _ = relay(socket, upstream, id, state.stop).await;
        }))
}
