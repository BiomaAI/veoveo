//! Binary stock-CLI bytes with authenticated internal lease controls. The public
//! edge consumes those controls because the stock client treats text as gRPC data.
use crate::{MAX_CONTROL_BYTES, MAX_MESSAGE_BYTES, Result, TransportError, Upstream, deadline};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use reqwest_websocket::Message as UpstreamMessage;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use veoveo_computers_contract::TerminalServerControl;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliRelayMode {
    /// Preserve service control bytes and deadlines for the next trusted relay.
    Internal,
    /// Strip every lease control before the unmodified stock CLI sees the stream.
    PublicClient,
}

/// Call only after an authenticated, fixed-route upgrade using the narrow CLI
/// grant. The worker owns grant renewal and the restricted gRPC method facade.
pub async fn relay_cli(
    downstream: WebSocket,
    upstream: Upstream,
    mode: CliRelayMode,
    stop: CancellationToken,
) -> Result<()> {
    let (mut guard, expiry) = deadline::Deadline::new();
    let (ready, mut admitted) = watch::channel(false);
    let (mut down_sink, mut down_stream) = downstream.split();
    let (mut up_sink, mut up_stream) = upstream.0.split();
    let (to_upstream, mut input) = mpsc::channel(2);
    let (to_downstream, mut output) = mpsc::channel(2);
    let receive_input = async {
        while let Some(message) = down_stream.next().await {
            let message = match message.map_err(|_| TransportError::Interrupted)? {
                Message::Binary(bytes) if !bytes.is_empty() && bytes.len() <= MAX_MESSAGE_BYTES => {
                    UpstreamMessage::Binary(bytes)
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => return Ok(()),
                _ => return Err(TransportError::Protocol),
            };
            to_upstream
                .send(message)
                .await
                .map_err(|_| TransportError::Interrupted)?;
        }
        Ok(())
    };
    let send_input = async {
        // The CLI immediately writes its HTTP/2 preface. Buffer it within the
        // normal queue while waiting for the worker's initial authority window.
        admitted
            .wait_for(|ready| *ready)
            .await
            .map_err(|_| TransportError::Interrupted)?;
        while let Some(message) = input.recv().await {
            if *expiry.borrow() <= tokio::time::Instant::now() {
                return Err(TransportError::Expired);
            }
            up_sink
                .send(message)
                .await
                .map_err(|_| TransportError::Interrupted)?;
        }
        Ok(())
    };
    let receive_output = async {
        while let Some(message) = up_stream.next().await {
            let message = match message.map_err(|_| TransportError::Interrupted)? {
                UpstreamMessage::Text(text) => {
                    if text.len() > MAX_CONTROL_BYTES {
                        return Err(TransportError::Protocol);
                    }
                    let control: TerminalServerControl =
                        serde_json::from_str(&text).map_err(|_| TransportError::Protocol)?;
                    if !matches!(
                        control,
                        TerminalServerControl::Ready(_) | TerminalServerControl::Lease(_)
                    ) {
                        return Err(TransportError::Protocol);
                    }
                    guard.control(&control)?;
                    ready.send_replace(true);
                    if mode == CliRelayMode::PublicClient {
                        continue;
                    }
                    Message::Text(text.into())
                }
                UpstreamMessage::Binary(bytes)
                    if guard.ready && !bytes.is_empty() && bytes.len() <= MAX_MESSAGE_BYTES =>
                {
                    Message::Binary(bytes)
                }
                UpstreamMessage::Ping(_) | UpstreamMessage::Pong(_) => continue,
                UpstreamMessage::Close { .. } => return Ok(()),
                _ => return Err(TransportError::Protocol),
            };
            to_downstream
                .send(message)
                .await
                .map_err(|_| TransportError::Interrupted)?;
        }
        Ok(())
    };
    let send_output = async {
        while let Some(message) = output.recv().await {
            if *expiry.borrow() <= tokio::time::Instant::now() {
                return Err(TransportError::Expired);
            }
            down_sink
                .send(message)
                .await
                .map_err(|_| TransportError::Interrupted)?;
        }
        Ok(())
    };
    tokio::select! {
        biased;
        _ = stop.cancelled() => Err(TransportError::Interrupted),
        _ = deadline::expired(expiry.clone()) => Err(TransportError::Expired),
        result = receive_input => result,
        result = send_input => result,
        result = receive_output => result,
        result = send_output => result,
    }
}
