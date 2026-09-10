use super::facade::Restricted;
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt, stream};
use std::convert::Infallible;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::watch,
};
use veoveo_computers::api::{
    TERMINAL_VERSION, TerminalLease, TerminalReady, TerminalReadyKind, TerminalServerControl,
};
use veoveo_computers_runtime::{AttachmentLease, protocol::v1::open_shell_server::OpenShellServer};

pub(super) async fn serve(
    mut socket: WebSocket,
    facade: Restricted,
    lease: AttachmentLease,
    mut updates: watch::Receiver<Option<TerminalLease>>,
) -> Result<(), ()> {
    let ready = TerminalServerControl::Ready(TerminalReady {
        version: TERMINAL_VERSION,
        kind: TerminalReadyKind::Ready,
        expires_at: lease.expires_at().map_err(|_| ())?.into(),
    });
    socket.send(control(ready)?).await.map_err(|_| ())?;
    let (tunnel, service_io) = tokio::io::duplex(65536);
    let (mut read, mut write) = tokio::io::split(tunnel);
    let (mut output, mut input) = socket.split();
    let incoming =
        stream::once(async move { Ok::<_, Infallible>(service_io) }).chain(stream::pending());
    let grpc = tonic::transport::Server::builder()
        .max_concurrent_streams(16)
        .add_service(
            OpenShellServer::new(facade)
                .max_decoding_message_size(1024 * 1024)
                .max_encoding_message_size(1024 * 1024),
        )
        .serve_with_incoming(incoming);
    let upload = async {
        while let Some(Ok(message)) = input.next().await {
            let bytes = match message {
                Message::Binary(bytes) if !bytes.is_empty() && bytes.len() <= 65536 => bytes,
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => return Ok(()),
                _ => return Err(()),
            };
            write.write_all(&bytes).await.map_err(|_| ())?;
        }
        Ok(())
    };
    let download = async {
        let mut buffer = vec![0; 32768];
        loop {
            let message = tokio::select! {
                biased;
                update = updates.changed() => {
                    update.map_err(|_| ())?;
                    let update = updates.borrow_and_update().clone().ok_or(())?;
                    control(TerminalServerControl::Lease(update))?
                }
                size = read.read(&mut buffer) => {
                    let size = size.map_err(|_| ())?;
                    if size == 0 { return Ok(()); }
                    Message::Binary(buffer[..size].to_vec().into())
                }
            };
            output.send(message).await.map_err(|_| ())?;
        }
    };
    tokio::select! {
        biased;
        _ = lease.closed() => Err(()),
        _ = grpc => Err(()),
        result = upload => result,
        result = download => result,
    }
}
fn control(value: TerminalServerControl) -> Result<Message, ()> {
    let text = serde_json::to_string(&value).map_err(|_| ())?;
    if text.len() > 1024 {
        return Err(());
    }
    Ok(Message::Text(text.into()))
}
