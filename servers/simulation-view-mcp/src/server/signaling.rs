use std::{sync::Arc, time::Duration};

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
use veoveo_mcp_contract::LiveViewId;

use crate::{
    contract::SimulationViewError, mcp::SimulationViewMcpState, state::SimulationViewService, uris,
};

const TOKEN_PROTOCOL_PREFIX: &str = "authorization.bearer.";
const SESSION_PROTOCOL_PREFIX: &str = "x-nv-sessionid.";
const MAX_SIGNALING_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct SignalingState {
    service: Arc<SimulationViewService>,
    mcp: Arc<SimulationViewMcpState>,
    upstream: Url,
    public_media_port_base: u16,
}

impl SignalingState {
    pub(super) fn new(
        service: Arc<SimulationViewService>,
        mcp: Arc<SimulationViewMcpState>,
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
            "renderer signaling URL must be a credential-free internal ws URL"
        );
        Ok(Self {
            service,
            mcp,
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
    let live_view_id = session_protocol
        .and_then(|protocol| protocol.strip_prefix(SESSION_PROTOCOL_PREFIX))
        .and_then(|value| value.parse::<LiveViewId>().ok());
    let (Some(token), Some(session_protocol), Some(live_view_id)) =
        (token, session_protocol.cloned(), live_view_id)
    else {
        return (
            StatusCode::UNAUTHORIZED,
            "live-view token and session protocols are required",
        )
            .into_response();
    };
    let authorized = match state.service.authorize_signaling(&live_view_id, token) {
        Ok(authorized) => authorized,
        Err(error) => return signaling_error(error),
    };
    let Some(render_slot) = authorized
        .endpoint
        .media_port
        .checked_sub(state.public_media_port_base)
    else {
        state.service.disconnect_signaling(&live_view_id);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "live-view media slot is invalid",
        )
            .into_response();
    };
    let upstream_url = match upstream_url(&state.upstream, &uri, render_slot) {
        Ok(url) => url,
        Err(status) => {
            state.service.disconnect_signaling(&live_view_id);
            return status.into_response();
        }
    };
    upgrade
        .max_message_size(MAX_SIGNALING_MESSAGE_BYTES)
        .protocols([session_protocol.clone()])
        .on_upgrade(move |socket| {
            bridge(
                state,
                authorized.session_id,
                live_view_id,
                session_protocol,
                upstream_url,
                socket,
            )
        })
}

async fn bridge(
    state: SignalingState,
    session_id: veoveo_mcp_contract::LiveSessionId,
    live_view_id: LiveViewId,
    session_protocol: String,
    upstream_url: Url,
    downstream: WebSocket,
) {
    let result = bridge_inner(
        &state,
        &live_view_id,
        &session_protocol,
        upstream_url,
        downstream,
    )
    .await;
    state.service.disconnect_signaling(&live_view_id);
    state
        .mcp
        .subscriptions
        .notify_resource_updated(uris::stream(&session_id, &live_view_id))
        .await;
    state
        .mcp
        .subscriptions
        .notify_resource_updated(uris::streams(&session_id))
        .await;
    if let Err(error) = result {
        tracing::warn!(%live_view_id, %error, "Simulation View signaling bridge closed");
    }
}

async fn bridge_inner(
    state: &SignalingState,
    live_view_id: &LiveViewId,
    session_protocol: &str,
    upstream_url: Url,
    downstream: WebSocket,
) -> anyhow::Result<()> {
    let mut request = upstream_url.as_str().into_client_request()?;
    request.headers_mut().insert(
        HeaderName::from_static("sec-websocket-protocol"),
        HeaderValue::from_str(session_protocol)?,
    );
    let (upstream, response) = connect_async(request).await?;
    anyhow::ensure!(
        response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|value| value.to_str().ok())
            == Some(session_protocol),
        "renderer selected an unexpected signaling protocol"
    );
    let (mut downstream_sink, mut downstream_source) = downstream.split();
    let (mut upstream_sink, mut upstream_source) = upstream.split();
    let downstream_to_upstream = async {
        while let Some(message) = downstream_source.next().await {
            let message = message?;
            if let Some(message) = to_upstream(message) {
                upstream_sink.send(message).await?;
            }
        }
        anyhow::Ok(())
    };
    let upstream_to_downstream = async {
        while let Some(message) = upstream_source.next().await {
            let message = message?;
            if let Some(message) = to_downstream(message) {
                downstream_sink.send(message).await?;
            }
        }
        anyhow::Ok(())
    };
    let lease = async {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if !state.service.signaling_active(live_view_id) {
                return anyhow::Ok(());
            }
        }
    };
    tokio::select! {
        result = downstream_to_upstream => result,
        result = upstream_to_downstream => result,
        result = lease => result,
    }
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

fn upstream_url(
    base: &Url,
    public_uri: &axum::http::Uri,
    render_slot: u16,
) -> Result<Url, StatusCode> {
    let path = public_uri.path();
    let suffix = path
        .split_once("/signaling")
        .map(|(_, suffix)| suffix)
        .unwrap_or_default();
    if suffix.contains("..") {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut url = base.clone();
    let port = base
        .port()
        .and_then(|port| port.checked_add(render_slot))
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    url.set_port(Some(port))
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let base_path = base.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}{suffix}"));
    url.set_query(public_uri.query());
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

fn signaling_error(error: SimulationViewError) -> Response {
    let status = match error {
        SimulationViewError::LiveViewNotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::FORBIDDEN,
    };
    (status, "live-view signaling authorization failed").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signaling_path_prefix_is_preserved_without_traversal() {
        let base = Url::parse("ws://renderer:49100/webrtc").unwrap();
        let public: axum::http::Uri = "/simulation-view/signaling/client?quality=high"
            .parse()
            .unwrap();
        assert_eq!(
            upstream_url(&base, &public, 2).unwrap().as_str(),
            "ws://renderer:49102/webrtc/client?quality=high"
        );
        let traversal: axum::http::Uri = "/simulation-view/signaling/../admin".parse().unwrap();
        assert!(upstream_url(&base, &traversal, 0).is_err());
    }

    #[test]
    fn nvidia_client_protocols_carry_the_live_token_and_session() {
        let protocols = parse_protocols(
            "authorization.bearer.secret-token, \
             x-nv-sessionid.stream-019fa2fa-a651-7ba2-ac7a-927e95101e5b",
        );
        assert_eq!(
            protocols
                .iter()
                .find_map(|protocol| protocol.strip_prefix(TOKEN_PROTOCOL_PREFIX)),
            Some("secret-token")
        );
        assert_eq!(
            protocols
                .iter()
                .find_map(|protocol| protocol.strip_prefix(SESSION_PROTOCOL_PREFIX)),
            Some("stream-019fa2fa-a651-7ba2-ac7a-927e95101e5b")
        );
    }
}
