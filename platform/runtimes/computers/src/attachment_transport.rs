//! One physically closable mTLS connection per attachment; no extra byte copy.
use crate::{Result, RuntimeFailure, client::Client};
use hyper_util::rt::TokioIo;
use std::{
    io,
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex},
};
use tonic::transport::Endpoint;

#[derive(Default)]
struct State {
    socket: Option<TcpStream>,
    started: bool,
    closed: bool,
}
pub(crate) struct TransportGuard(Arc<Mutex<State>>);
impl Drop for TransportGuard {
    fn drop(&mut self) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        if let Some(socket) = state.socket.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}

pub(crate) async fn connect(
    endpoint: &Endpoint,
    address: &str,
) -> Result<(Client, TransportGuard)> {
    let state = Arc::new(Mutex::new(State::default()));
    let guard = TransportGuard(state.clone());
    let address = address.to_owned();
    let connector = tower::service_fn(move |_| {
        let state = state.clone();
        let address = address.clone();
        async move {
            {
                let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                if state.closed || state.started {
                    return Err(io::Error::other("attachment transport ended"));
                }
                state.started = true;
            }
            let stream = tokio::net::TcpStream::connect(&address).await?;
            stream.set_nodelay(true)?;
            let stream = stream.into_std()?;
            let closer = stream.try_clone()?;
            {
                let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                if state.closed {
                    return Err(io::Error::other("attachment transport ended"));
                }
                state.socket = Some(closer);
            }
            Ok(TokioIo::new(tokio::net::TcpStream::from_std(stream)?))
        }
    });
    let channel = endpoint
        .connect_with_connector(connector)
        .await
        .map_err(|_| RuntimeFailure::Unavailable)?;
    Ok((
        Client::new(channel)
            .max_decoding_message_size(1024 * 1024)
            .max_encoding_message_size(1024 * 1024),
        guard,
    ))
}
