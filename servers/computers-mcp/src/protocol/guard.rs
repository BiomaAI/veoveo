//! Deadline and cancellation remain polled while either I/O or renewal is blocked.
use futures::{Stream, StreamExt};
use rmcp::ErrorData;
use std::time::Instant;

pub(super) async fn run<F: Future<Output = Result<Instant, ErrorData>>>(
    deadline: Instant,
    mut changes: impl Stream<Item = veoveo_computers::Result<()>> + Unpin,
    cancelled: impl Future<Output = ()>,
    pump: impl Future<Output = Result<(), ErrorData>>,
    mut refresh: impl FnMut() -> F,
) -> Result<(), ErrorData> {
    tokio::pin!(pump, cancelled);
    let expires = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
    tokio::pin!(expires);
    let renew = tokio::time::sleep_until(crate::io_guard::renew_at(deadline));
    tokio::pin!(renew);
    loop {
        tokio::select! {
            biased;
            _ = &mut expires => return Err(super::auth::forbidden()),
            _ = &mut cancelled => return Ok(()),
            result = async {
                tokio::select! {
                    biased;
                    change = changes.next() => if !matches!(change, Some(Ok(()))) { return Err(super::auth::unavailable()); },
                    _ = &mut renew => {},
                }
                refresh().await
            } => {
                let deadline = result?;
                expires.as_mut().reset(deadline.into());
                renew.as_mut().reset(crate::io_guard::renew_at(deadline));
            }
            result = &mut pump => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::pending,
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };

    #[tokio::test]
    async fn expiry_and_cancellation_interrupt_a_blocked_authority_read_and_sink() {
        for cancel in [false, true] {
            let entered = AtomicBool::new(false);
            let now = Instant::now();
            let deadline = now + Duration::from_millis(if cancel { 10_000 } else { 30 });
            let result = tokio::time::timeout(
                Duration::from_secs(1),
                run(
                    deadline,
                    futures::stream::iter([Ok(())]).chain(futures::stream::pending()),
                    async {
                        if cancel {
                            tokio::time::sleep(Duration::from_millis(30)).await;
                        } else {
                            pending::<()>().await;
                        }
                    },
                    pending(),
                    || async {
                        entered.store(true, Ordering::Relaxed);
                        pending().await
                    },
                ),
            )
            .await
            .expect("blocked renewal concealed deadline or cancellation");
            assert!(entered.load(Ordering::Relaxed));
            assert_eq!(result.is_ok(), cancel);
        }
    }
}
