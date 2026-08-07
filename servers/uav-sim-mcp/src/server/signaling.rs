use std::sync::Arc;

use axum::{
    extract::{
        OriginalUri, State,
        ws::{CloseFrame, Message as DownstreamMessage, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header::SEC_WEBSOCKET_PROTOCOL},
    response::{IntoResponse, Response},
};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message as UpstreamMessage,
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue},
    },
};
use url::Url;
use veoveo_mcp_contract::{LiveViewId, SubscriptionHub};

use super::live_view::{LeaseSignal, LiveViewError, LiveViewService};
use crate::uris;

const TOKEN_PROTOCOL_PREFIX: &str = "authorization.bearer.";
const SESSION_PROTOCOL_PREFIX: &str = "x-nv-sessionid.";
const LIVE_VIEW_QUERY: &str = "live_view_id";
const MAX_SIGNALING_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct SignalingState {
    service: Arc<LiveViewService>,
    subscriptions: Arc<SubscriptionHub>,
    upstream: Url,
    public_media_port_base: u16,
}

impl SignalingState {
    pub(super) fn new(
        service: Arc<LiveViewService>,
        subscriptions: Arc<SubscriptionHub>,
        upstream: &str,
        public_media_port_base: u16,
    ) -> anyhow::Result<Self> {
        let upstream = Url::parse(upstream)?;
        anyhow::ensure!(
            upstream.scheme() == "ws"
                && upstream.host_str().is_some()
                && upstream.username().is_empty()
                && upstream.password().is_none()
                && upstream.port().is_some()
                && upstream.query().is_none()
                && upstream.fragment().is_none(),
            "simulator signaling URL must be a credential-free internal ws URL with an explicit base port"
        );
        Ok(Self {
            service,
            subscriptions,
            upstream,
            public_media_port_base,
        })
    }
}

pub(super) async fn upgrade(
    State(state): State<SignalingState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let protocols = headers
        .get(SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .map(parse_protocols)
        .unwrap_or_default();
    let token = protocols
        .iter()
        .find_map(|protocol| protocol.strip_prefix(TOKEN_PROTOCOL_PREFIX))
        .filter(|token| !token.is_empty());
    let session_protocol = protocols
        .iter()
        .find(|protocol| protocol.starts_with(SESSION_PROTOCOL_PREFIX));
    let live_view_id = live_view_id(&uri);
    let (Some(token), Some(session_protocol), Some(live_view_id)) =
        (token, session_protocol.cloned(), live_view_id)
    else {
        return (
            StatusCode::UNAUTHORIZED,
            "live-view token, product session, and identity are required",
        )
            .into_response();
    };
    let authorized = match state
        .service
        .authorize_signaling(&live_view_id, token)
        .await
    {
        Ok(authorized) => authorized,
        Err(error) => return signaling_error(error),
    };
    let Some(slot) = authorized
        .state
        .endpoint
        .media_port
        .checked_sub(state.public_media_port_base)
    else {
        state.service.disconnect_signaling(&live_view_id).await;
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "live-view media slot is invalid",
        )
            .into_response();
    };
    let expected_session_protocol = format!(
        "{SESSION_PROTOCOL_PREFIX}{}",
        authorized.state.stream_product_id
    );
    if session_protocol != expected_session_protocol {
        state.service.disconnect_signaling(&live_view_id).await;
        return (
            StatusCode::FORBIDDEN,
            "live-view stream-product session does not match the authorized product",
        )
            .into_response();
    }
    let upstream_url = match upstream_url(
        &state.upstream,
        &uri,
        slot,
        &authorized.state.stream_product_id,
    ) {
        Ok(url) => url,
        Err(status) => {
            state.service.disconnect_signaling(&live_view_id).await;
            return status.into_response();
        }
    };
    tracing::info!(
        %live_view_id,
        stream_product_id = %authorized.state.stream_product_id,
        slot,
        "accepted simulator-hosted live-view signaling"
    );
    upgrade
        .max_message_size(MAX_SIGNALING_MESSAGE_BYTES)
        .protocols([session_protocol.clone()])
        .on_upgrade(move |socket| {
            bridge(
                state,
                authorized.state.session_id,
                live_view_id,
                session_protocol,
                upstream_url,
                socket,
                authorized.events,
            )
        })
}

async fn bridge(
    state: SignalingState,
    session_id: veoveo_mcp_contract::LiveSessionId,
    live_view_id: LiveViewId,
    upstream_session_protocol: String,
    upstream_url: Url,
    downstream: WebSocket,
    lease_events: tokio::sync::watch::Receiver<LeaseSignal>,
) {
    let result = bridge_inner(
        &upstream_session_protocol,
        upstream_url,
        downstream,
        lease_events,
    )
    .await;
    state.service.disconnect_signaling(&live_view_id).await;
    state
        .subscriptions
        .notify_resource_updated(uris::live_view(&session_id, &live_view_id))
        .await;
    state
        .subscriptions
        .notify_resource_updated(uris::live_views(&session_id))
        .await;
    if let Err(error) = result {
        tracing::warn!(%live_view_id, %error, "simulator-hosted signaling bridge closed");
    }
}

async fn bridge_inner(
    upstream_session_protocol: &str,
    upstream_url: Url,
    downstream: WebSocket,
    lease_events: tokio::sync::watch::Receiver<LeaseSignal>,
) -> anyhow::Result<()> {
    let mut request = upstream_url.as_str().into_client_request()?;
    request.headers_mut().insert(
        HeaderName::from_static("sec-websocket-protocol"),
        HeaderValue::from_str(upstream_session_protocol)?,
    );
    let (upstream, response) = connect_async(request).await?;
    anyhow::ensure!(
        response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|value| value.to_str().ok())
            == Some(upstream_session_protocol),
        "simulator selected an unexpected signaling protocol"
    );
    let (mut downstream_sink, mut downstream_source) = downstream.split();
    let (mut upstream_sink, mut upstream_source) = upstream.split();
    let downstream_to_upstream = async {
        while let Some(message) = downstream_source.next().await {
            if let Some(message) = to_upstream(message?) {
                upstream_sink.send(message).await?;
            }
        }
        anyhow::Ok(())
    };
    let upstream_to_downstream = async {
        while let Some(message) = upstream_source.next().await {
            if let Some(message) = to_downstream(message?) {
                downstream_sink.send(message).await?;
            }
        }
        anyhow::Ok(())
    };
    tokio::select! {
        result = downstream_to_upstream => result,
        result = upstream_to_downstream => result,
        result = wait_for_lease_end(lease_events) => result,
    }
}

async fn wait_for_lease_end(
    mut events: tokio::sync::watch::Receiver<LeaseSignal>,
) -> anyhow::Result<()> {
    loop {
        let current = events.borrow().clone();
        if !current.active || current.expires_at <= chrono::Utc::now() {
            return Ok(());
        }
        let remaining = (current.expires_at - chrono::Utc::now())
            .to_std()
            .unwrap_or_default();
        tokio::select! {
            _ = tokio::time::sleep(remaining) => return Ok(()),
            changed = events.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
            }
        }
    }
}

fn live_view_id(uri: &axum::http::Uri) -> Option<LiveViewId> {
    let mut values = url::form_urlencoded::parse(uri.query()?.as_bytes())
        .filter(|(name, _)| name == LIVE_VIEW_QUERY)
        .map(|(_, value)| value);
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    value.parse().ok()
}

fn upstream_url(
    base: &Url,
    public_uri: &axum::http::Uri,
    slot: u16,
    stream_product_id: &veoveo_mcp_contract::LiveStreamProductId,
) -> Result<Url, StatusCode> {
    let suffix = public_uri
        .path()
        .split_once("/signaling")
        .map(|(_, suffix)| suffix)
        .unwrap_or_default();
    if suffix.contains("..") {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut url = base.clone();
    let port = base
        .port()
        .and_then(|port| port.checked_add(slot))
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    url.set_port(Some(port))
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let base_path = base.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}{suffix}"));
    if let Some(query) = public_uri.query() {
        let pairs = url::form_urlencoded::parse(query.as_bytes())
            .filter(|(name, _)| name != LIVE_VIEW_QUERY && name != "pairing_id")
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }
    }
    url.query_pairs_mut()
        .append_pair("pairing_id", stream_product_id.as_str());
    Ok(url)
}

fn parse_protocols(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn to_upstream(message: DownstreamMessage) -> Option<UpstreamMessage> {
    match message {
        DownstreamMessage::Text(value) => Some(UpstreamMessage::Text(value.to_string().into())),
        DownstreamMessage::Binary(value) => Some(UpstreamMessage::Binary(value.to_vec().into())),
        DownstreamMessage::Ping(value) => Some(UpstreamMessage::Ping(value.to_vec().into())),
        DownstreamMessage::Pong(value) => Some(UpstreamMessage::Pong(value.to_vec().into())),
        DownstreamMessage::Close(frame) => Some(UpstreamMessage::Close(frame.map(|frame| {
            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.to_string().into(),
            }
        }))),
    }
}

fn to_downstream(message: UpstreamMessage) -> Option<DownstreamMessage> {
    match message {
        UpstreamMessage::Text(value) => Some(DownstreamMessage::Text(value.to_string().into())),
        UpstreamMessage::Binary(value) => Some(DownstreamMessage::Binary(value.to_vec().into())),
        UpstreamMessage::Ping(value) => Some(DownstreamMessage::Ping(value.to_vec().into())),
        UpstreamMessage::Pong(value) => Some(DownstreamMessage::Pong(value.to_vec().into())),
        UpstreamMessage::Close(frame) => {
            Some(DownstreamMessage::Close(frame.map(|frame| CloseFrame {
                code: u16::from(frame.code),
                reason: frame.reason.to_string().into(),
            })))
        }
        UpstreamMessage::Frame(_) => None,
    }
}

fn signaling_error(error: LiveViewError) -> Response {
    let status = match error {
        LiveViewError::ViewNotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::FORBIDDEN,
    };
    (status, "live-view signaling authorization failed").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_removes_private_identity_and_selects_capacity_slot() {
        let base = Url::parse("ws://127.0.0.1:49100/webrtc").unwrap();
        let public: axum::http::Uri =
            "/uav-sim/signaling/sign_in?live_view_id=view-1&pairing_id=secret"
                .parse()
                .unwrap();
        let product = veoveo_mcp_contract::LiveStreamProductId::new("product-follow").unwrap();
        assert_eq!(
            upstream_url(&base, &public, 2, &product).unwrap().as_str(),
            "ws://127.0.0.1:49102/webrtc/sign_in?pairing_id=product-follow"
        );
    }
}
