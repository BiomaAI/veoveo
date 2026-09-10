//! Narrow CLI credentials bypass OAuth JWT decoding, never domain authorization.
use super::{ComputersState, Fault};
use axum::{
    Router,
    extract::{Path, RawQuery, State, ws::WebSocketUpgrade},
    http::{HeaderMap, header},
    response::Response,
    routing::get,
};
use serde::Deserialize;
use uuid::Uuid;
use veoveo_computers_transport::{
    CliCredentialFraming, CliRelayMode, CliUpstreamRequest, MAX_MESSAGE_BYTES, cli_authorization,
    relay_cli,
};
use veoveo_mcp_contract::{GatewayProfileId, ServerSlug};

#[derive(Deserialize)]
struct Route {
    profile: GatewayProfileId,
    id: Option<Uuid>,
}
pub(super) fn router(state: ComputersState) -> Router {
    Router::new()
        .route("/computers/{profile}/cli/_ws_tunnel", get(upgrade))
        .route("/computers/{profile}/cli/{id}/_ws_tunnel", get(upgrade))
        .with_state(state)
}
async fn upgrade(
    State(state): State<ComputersState>,
    Path(route): Path<Route>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, Fault> {
    if route.id.is_some_and(|id| id.is_nil()) {
        return Err(Fault::invalid());
    }
    let authorization =
        cli_authorization(&headers, CliCredentialFraming::Internal, query.as_deref())
            .map_err(|_| Fault::denied())?;
    let catalog = state.catalog.current();
    let server = ServerSlug::new("computers").expect("static server");
    let (_, _, manifest) = catalog
        .profile_server(&route.profile, &server)
        .ok_or_else(Fault::missing)?;
    let slot = state
        .slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Fault::unavailable())?;
    let suffix = route.id.map(|id| format!("/{id}")).unwrap_or_default();
    let mut url =
        url::Url::parse(manifest.upstream.url.as_str()).map_err(|_| Fault::unavailable())?;
    url.set_path(&format!(
        "{}/cli/{}{suffix}/_ws_tunnel",
        manifest.mount_path.as_str().trim_end_matches('/'),
        route.profile
    ));
    url.set_query(None);
    url.set_fragment(None);
    let upstream = tokio::select! {
        biased;
        _ = state.stop.cancelled() => return Err(Fault::unavailable()),
        result = async {
            let client = state.upstream.websocket_client(&catalog, manifest).await.map_err(|_| Fault::unavailable())?;
            client.connect_cli(CliUpstreamRequest { url, authorization, host: None }).await.map_err(|_| Fault::denied())
        } => result?,
    };
    let mut response = ws
        .max_frame_size(MAX_MESSAGE_BYTES)
        .max_message_size(MAX_MESSAGE_BYTES)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_MESSAGE_BYTES * 2)
        .on_upgrade(move |socket| async move {
            let _slot = slot;
            let _ = relay_cli(socket, upstream, CliRelayMode::Internal, state.stop).await;
        });
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    Ok(response)
}
