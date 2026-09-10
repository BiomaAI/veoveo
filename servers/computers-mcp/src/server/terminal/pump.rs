use super::authority::Activity;
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use veoveo_computers::api::*;
use veoveo_computers_runtime::{Terminal, TerminalOutput, TerminalSize};

pub(super) async fn run(
    mut socket: WebSocket,
    mut terminal: Terminal,
    activity: &Activity,
    mut updates: tokio::sync::watch::Receiver<Option<TerminalLease>>,
) -> Result<(), ()> {
    let ready = TerminalServerControl::Ready(TerminalReady {
        version: TERMINAL_VERSION,
        kind: TerminalReadyKind::Ready,
        expires_at: terminal.expires_at().map_err(|_| ())?.into(),
    });
    socket
        .send(Message::Text(
            serde_json::to_string(&ready).map_err(|_| ())?.into(),
        ))
        .await
        .map_err(|_| ())?;
    let (mut sink, mut source) = socket.split();
    let input = terminal.input();
    let replayed = AtomicBool::new(false);
    let output = async {
        loop {
            let output = tokio::select! {
                biased;
                changed = updates.changed() => {
                    changed.map_err(|_| ())?;
                    let update = updates.borrow_and_update().clone().ok_or(())?;
                    sink.send(Message::Text(serde_json::to_string(&TerminalServerControl::Lease(update)).map_err(|_| ())?.into())).await.map_err(|_| ())?;
                    continue;
                }
                output = terminal.read() => output.map_err(|_| ())?,
            };
            let Some(output) = output else {
                return Ok::<_, ()>(());
            };
            let fence = matches!(output, TerminalOutput::ReplayComplete);
            let message = match output {
                TerminalOutput::Data(bytes) => Message::Binary(bytes.into()),
                TerminalOutput::ReplayComplete => {
                    if replayed.load(Ordering::Acquire) {
                        return Err(());
                    }
                    Message::Text(
                        serde_json::to_string(&TerminalServerControl::ReplayComplete(
                            TerminalReplayComplete {
                                kind: TerminalReplayCompleteKind::ReplayComplete,
                            },
                        ))
                        .map_err(|_| ())?
                        .into(),
                    )
                }
            };
            sink.send(message).await.map_err(|_| ())?;
            if fence {
                replayed.store(true, Ordering::Release);
            }
        }
    };
    let receive = async {
        while let Some(message) = source.next().await {
            match message.map_err(|_| ())? {
                Message::Binary(bytes) => {
                    if !replayed.load(Ordering::Acquire) {
                        return Err(());
                    }
                    input.write(&bytes).await.map_err(|_| ())?;
                    activity.record();
                }
                Message::Text(text) => {
                    if text.len() > 1024 {
                        return Err(());
                    }
                    let resize: TerminalResize = serde_json::from_str(&text).map_err(|_| ())?;
                    input
                        .resize(TerminalSize::new(resize.cols, resize.rows).map_err(|_| ())?)
                        .await
                        .map_err(|_| ())?;
                }
                Message::Close(_) => return Ok(()),
                Message::Ping(_) | Message::Pong(_) => {}
            }
        }
        Ok::<_, ()>(())
    };
    tokio::select! {
        result = output => result,
        result = receive => result,
    }
}
