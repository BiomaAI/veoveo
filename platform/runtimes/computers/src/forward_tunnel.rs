//! A narrow SSH tunnel whose authority task runs independently of backpressure.
use crate::{
    AttachmentLease, MAX_CHUNK_BYTES, OpenShellRuntime, Result, RuntimeFailure, protocol::v1 as api,
};
use futures::{Stream, StreamExt, stream};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

pub struct ForwardTunnel {
    output: mpsc::Receiver<Result<api::TcpForwardFrame>>,
    worker: JoinHandle<()>,
    closed: Pin<Box<dyn Future<Output = ()> + Send>>,
    ended: bool,
}
impl Drop for ForwardTunnel {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
impl Stream for ForwardTunnel {
    type Item = std::result::Result<api::TcpForwardFrame, tonic::Status>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        if self.closed.as_mut().poll(cx).is_ready() {
            self.worker.abort();
            self.output.close();
            while self.output.try_recv().is_ok() {}
            self.ended = true;
            return Poll::Ready(Some(Err(tonic::Status::unauthenticated(
                "attachment authority ended",
            ))));
        }
        match self.output.poll_recv(cx) {
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
            Poll::Ready(Some(Err(_))) => {
                self.ended = true;
                Poll::Ready(Some(Err(tonic::Status::unavailable(
                    "SSH tunnel interrupted",
                ))))
            }
            Poll::Ready(None) => {
                self.ended = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pub(crate) async fn open<S>(
    runtime: OpenShellRuntime,
    sandbox: String,
    lease: AttachmentLease,
    input: S,
) -> Result<ForwardTunnel>
where
    S: Stream<Item = api::TcpForwardFrame> + Send + 'static,
{
    lease.check()?;
    let (output, receiver) = mpsc::channel(2);
    let (ready, admitted) = oneshot::channel();
    let worker_lease = lease.clone();
    let worker = tokio::spawn(async move {
        let mut ready = Some(ready);
        let work = async {
            let mut input = Box::pin(input);
            let init = tokio::time::timeout(Duration::from_secs(10), input.next())
                .await
                .map_err(|_| RuntimeFailure::TerminalFailed)?
                .ok_or(RuntimeFailure::TerminalBounds)?;
            validate_init(&init, &sandbox)?;
            let (mut client, _transport) =
                crate::attachment_transport::connect(&runtime.endpoint, &runtime.address).await?;
            let (send, receive) = mpsc::channel(1);
            send.send(init)
                .await
                .map_err(|_| RuntimeFailure::TerminalFailed)?;
            let upstream = stream::unfold(receive, |mut receive| async move {
                receive.recv().await.map(|frame| (frame, receive))
            });
            let mut response = tokio::time::timeout(
                Duration::from_secs(10),
                client.forward_tcp(tonic::Request::new(upstream)),
            )
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?
            .map_err(|_| RuntimeFailure::TerminalFailed)?
            .into_inner();
            ready
                .take()
                .unwrap()
                .send(Ok(()))
                .map_err(|_| RuntimeFailure::TerminalFailed)?;
            let upload = async {
                while let Some(frame) = input.next().await {
                    validate_data(&frame)?;
                    send.send(frame)
                        .await
                        .map_err(|_| RuntimeFailure::TerminalFailed)?;
                }
                Ok(())
            };
            let download = async {
                while let Some(frame) = response
                    .message()
                    .await
                    .map_err(|_| RuntimeFailure::TerminalFailed)?
                {
                    validate_data(&frame)?;
                    output
                        .send(Ok(frame))
                        .await
                        .map_err(|_| RuntimeFailure::TerminalFailed)?;
                }
                Ok(())
            };
            tokio::select! { result = upload => result, result = download => result }
        };
        let result = tokio::select! {
            biased;
            _ = worker_lease.closed() => Err(RuntimeFailure::LeaseExpired),
            result = work => result,
        };
        if let Some(ready) = ready {
            let _ = ready.send(result);
        }
        if let Err(error) = result {
            let _ = output.try_send(Err(error));
        }
    });
    let tunnel = ForwardTunnel {
        output: receiver,
        worker,
        closed: Box::pin(async move { lease.closed().await }),
        ended: false,
    };
    // Dropping the admission future also drops this owner and aborts both pumps.
    admitted
        .await
        .map_err(|_| RuntimeFailure::TerminalFailed)??;
    Ok(tunnel)
}

fn validate_init(frame: &api::TcpForwardFrame, sandbox: &str) -> Result<()> {
    let Some(api::tcp_forward_frame::Payload::Init(init)) = &frame.payload else {
        return Err(RuntimeFailure::TerminalBounds);
    };
    if init.sandbox_id != sandbox
        || init.service_id != format!("ssh-proxy:{sandbox}")
        || !matches!(init.target, Some(api::tcp_forward_init::Target::Ssh(_)))
        || !crate::remote_access::valid_token(&init.authorization_token)
    {
        return Err(RuntimeFailure::BindingMismatch);
    }
    Ok(())
}
fn validate_data(frame: &api::TcpForwardFrame) -> Result<()> {
    if matches!(&frame.payload, Some(api::tcp_forward_frame::Payload::Data(bytes)) if bytes.len() <= MAX_CHUNK_BYTES)
    {
        Ok(())
    } else {
        Err(RuntimeFailure::TerminalBounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssh_only_exact_target_and_data_bounds() {
        let mut init = api::TcpForwardInit {
            sandbox_id: "owned".into(),
            service_id: "ssh-proxy:owned".into(),
            target: Some(api::tcp_forward_init::Target::Ssh(api::SshRelayTarget {})),
            authorization_token: "opaque".into(),
        };
        let frame = |init| api::TcpForwardFrame {
            payload: Some(api::tcp_forward_frame::Payload::Init(init)),
        };
        assert!(validate_init(&frame(init.clone()), "owned").is_ok());
        assert!(validate_init(&frame(init.clone()), "other").is_err());
        init.service_id = "arbitrary-service".into();
        assert!(validate_init(&frame(init.clone()), "owned").is_err());
        init.service_id = "ssh-proxy:owned".into();
        init.target = None;
        assert!(validate_init(&frame(init.clone()), "owned").is_err());
        assert!(validate_data(&frame(init)).is_err());
        assert!(
            validate_data(&api::TcpForwardFrame {
                payload: Some(api::tcp_forward_frame::Payload::Data(vec![
                    0;
                    MAX_CHUNK_BYTES
                        + 1
                ]))
            })
            .is_err()
        );
    }
}
