use crate::{MAX_CONTROL_BYTES, MAX_MESSAGE_BYTES, Result, TransportError, Upstream, deadline};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use reqwest_websocket::Message as UpstreamMessage;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_computers_contract::{
    TERMINAL_VERSION, TerminalAttach, TerminalResize, TerminalServerControl,
};

struct Delivery {
    message: Message,
    replay_fence: bool,
}

/// Call only after current route authorization and an authenticated upstream
/// upgrade. No arbitrary target, cookie or provider credential is accepted here.
pub async fn relay(
    mut downstream: WebSocket,
    mut upstream: Upstream,
    computer: Uuid,
    stop: CancellationToken,
) -> Result<()> {
    let first = tokio::select! {
        biased;
        _ = stop.cancelled() => return Err(TransportError::Interrupted),
        result = tokio::time::timeout(Duration::from_secs(5), downstream.recv()) => result.map_err(|_| TransportError::Expired)?,
    };
    let Some(Ok(Message::Text(first))) = first else {
        return Err(TransportError::Protocol);
    };
    if first.len() > MAX_CONTROL_BYTES {
        return Err(TransportError::Protocol);
    }
    let attach: TerminalAttach =
        serde_json::from_str(&first).map_err(|_| TransportError::Protocol)?;
    if attach.version != TERMINAL_VERSION
        || attach.computer_id != computer
        || computer.is_nil()
        || !dimensions(attach.cols, attach.rows)
    {
        return Err(TransportError::Protocol);
    }
    drop(attach);
    tokio::time::timeout(
        Duration::from_secs(5),
        upstream.0.send(UpstreamMessage::Text(first.to_string())),
    )
    .await
    .map_err(|_| TransportError::Interrupted)?
    .map_err(|_| TransportError::Interrupted)?;
    drop(first);
    let (mut guard, expiry) = deadline::Deadline::new();
    let (mut down_sink, mut down_stream) = downstream.split();
    let (mut up_sink, mut up_stream) = upstream.0.split();
    let (to_upstream, mut input) = mpsc::channel(2);
    let (to_downstream, mut output) = mpsc::channel::<Delivery>(2);
    let replayed = AtomicBool::new(false);
    let receive_input = async {
        while let Some(message) = down_stream.next().await {
            let message = match message.map_err(|_| TransportError::Interrupted)? {
                Message::Binary(bytes) => {
                    if !replayed.load(Ordering::Acquire)
                        || bytes.is_empty()
                        || bytes.len() > MAX_MESSAGE_BYTES
                    {
                        return Err(TransportError::Protocol);
                    }
                    UpstreamMessage::Binary(bytes)
                }
                Message::Text(text) => {
                    if text.len() > MAX_CONTROL_BYTES {
                        return Err(TransportError::Protocol);
                    }
                    let resize: TerminalResize =
                        serde_json::from_str(&text).map_err(|_| TransportError::Protocol)?;
                    if !dimensions(resize.cols, resize.rows) {
                        return Err(TransportError::Protocol);
                    }
                    UpstreamMessage::Text(text.to_string())
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => return Ok(()),
            };
            to_upstream
                .send(message)
                .await
                .map_err(|_| TransportError::Interrupted)?;
        }
        Ok(())
    };
    let send_input = async {
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
            let (message, replay_fence) = match message.map_err(|_| TransportError::Interrupted)? {
                UpstreamMessage::Text(text) => {
                    if text.len() > MAX_CONTROL_BYTES {
                        return Err(TransportError::Protocol);
                    }
                    let control: TerminalServerControl =
                        serde_json::from_str(&text).map_err(|_| TransportError::Protocol)?;
                    guard.control(&control)?;
                    let fence = matches!(control, TerminalServerControl::ReplayComplete(_));
                    (Message::Text(text.into()), fence)
                }
                UpstreamMessage::Binary(bytes) => {
                    if !guard.ready || bytes.len() > MAX_MESSAGE_BYTES {
                        return Err(TransportError::Protocol);
                    }
                    (Message::Binary(bytes), false)
                }
                UpstreamMessage::Ping(_) | UpstreamMessage::Pong(_) => continue,
                UpstreamMessage::Close { .. } => return Ok(()),
            };
            to_downstream
                .send(Delivery {
                    message,
                    replay_fence,
                })
                .await
                .map_err(|_| TransportError::Interrupted)?;
        }
        Ok(())
    };
    let send_output = async {
        while let Some(delivery) = output.recv().await {
            if *expiry.borrow() <= tokio::time::Instant::now() {
                return Err(TransportError::Expired);
            }
            down_sink
                .send(delivery.message)
                .await
                .map_err(|_| TransportError::Interrupted)?;
            if delivery.replay_fence {
                replayed.store(true, Ordering::Release);
            }
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
fn dimensions(cols: u32, rows: u32) -> bool {
    (2..=500).contains(&cols) && (1..=200).contains(&rows)
}
