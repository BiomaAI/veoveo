use super::*;
use futures::StreamExt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::time::Instant;

#[derive(Clone, Default)]
pub(super) struct Probe {
    closed: Arc<AtomicBool>,
    sent: Arc<AtomicUsize>,
}
struct ResponseOwner(Probe, tonic::Streaming<api::TcpForwardFrame>);
impl Drop for ResponseOwner {
    fn drop(&mut self) {
        self.0.closed.store(true, Ordering::SeqCst);
    }
}
pub(super) fn response(
    probe: Probe,
    input: tonic::Streaming<api::TcpForwardFrame>,
) -> Response<BoxStream<api::TcpForwardFrame>> {
    Response::new(Box::pin(stream::unfold(
        ResponseOwner(probe, input),
        |owner| async move {
            // Hold the incoming stream without draining it to exercise upload pressure.
            let _ = &owner.1;
            let count = owner.0.sent.fetch_add(1, Ordering::SeqCst);
            assert!(
                count < 256,
                "bounded test must backpressure the native HTTP/2 body"
            );
            Some((
                Ok(api::TcpForwardFrame {
                    payload: Some(api::tcp_forward_frame::Payload::Data(vec![
                        7;
                        MAX_CHUNK_BYTES
                    ])),
                }),
                owner,
            ))
        },
    )))
}

fn init(sandbox: &str) -> api::TcpForwardFrame {
    api::TcpForwardFrame {
        payload: Some(api::tcp_forward_frame::Payload::Init(api::TcpForwardInit {
            sandbox_id: sandbox.into(),
            service_id: format!("ssh-proxy:{sandbox}"),
            target: Some(api::tcp_forward_init::Target::Ssh(api::SshRelayTarget {})),
            authorization_token: "local-fixture-token".into(),
        })),
    }
}

#[tokio::test]
async fn forwarding_rejects_another_target_before_native_admission() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    let access = running
        .runtime
        .open_shell_access(&binding(), running.lease(Duration::from_secs(30)))
        .await
        .unwrap();
    assert!(matches!(
        access.forward_tcp(stream::iter([init("other")])).await,
        Err(RuntimeFailure::BindingMismatch)
    ));
}

#[tokio::test]
async fn forward_tunnel_renews_and_revokes_under_bidirectional_backpressure() {
    let running = Running::start().await;
    let probe = Probe::default();
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.forward_probe = Some(probe.clone());
    }
    let (authority, lease) = LeaseAuthority::issue(Instant::now(), Duration::from_secs(1)).unwrap();
    let access = running
        .runtime
        .open_shell_access(&binding(), lease)
        .await
        .unwrap();
    let (send, receive) = tokio::sync::mpsc::channel(1);
    send.send(init("sandbox-1")).await.unwrap();
    let input = stream::unfold(receive, |mut receive| async move {
        receive.recv().await.map(|frame| (frame, receive))
    });
    let mut tunnel = access.forward_tcp(input).await.unwrap();
    let writer = tokio::spawn(async move {
        for _ in 0..256 {
            if send
                .send(api::TcpForwardFrame {
                    payload: Some(api::tcp_forward_frame::Payload::Data(vec![
                        9;
                        MAX_CHUNK_BYTES
                    ])),
                })
                .await
                .is_err()
            {
                return;
            }
        }
        panic!("upload did not reach backpressure");
    });
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        authority
            .renew(Instant::now(), Duration::from_secs(1))
            .unwrap();
    }
    assert!(!writer.is_finished());
    assert!(!probe.closed.load(Ordering::SeqCst));
    let count = probe.sent.load(Ordering::SeqCst);
    assert!(count > 2);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        probe.sent.load(Ordering::SeqCst),
        count,
        "unread output must stop at transport bounds"
    );
    authority.revoke();
    tokio::time::timeout(Duration::from_secs(3), writer)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !probe.closed.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // Buffered output cannot escape after the lease has ended.
    assert_eq!(
        tunnel.next().await.unwrap().err().unwrap().code(),
        tonic::Code::Unauthenticated
    );
    assert!(tunnel.next().await.is_none());
    running.runtime.ready().await.unwrap();
    assert_eq!(running.fake.0.lock().unwrap().stops, 0);
}
