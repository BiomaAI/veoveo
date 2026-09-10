//! Deadline and cancellation remain polled while either I/O or renewal is blocked.
use rmcp::ErrorData;
use std::time::{Duration, Instant};

pub(super) async fn run<F: Future<Output = Result<Instant, ErrorData>>>(
    deadline: Instant,
    check_every: Duration,
    cancelled: impl Future<Output = ()>,
    pump: impl Future<Output = Result<(), ErrorData>>,
    mut refresh: impl FnMut() -> F,
) -> Result<(), ErrorData> {
    tokio::pin!(pump, cancelled);
    let expires = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
    tokio::pin!(expires);
    let mut recheck = tokio::time::interval(check_every);
    recheck.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    recheck.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = &mut expires => return Err(super::auth::forbidden()),
            _ = &mut cancelled => return Ok(()),
            result = &mut pump => return result,
            result = async { recheck.tick().await; refresh().await } => {
                expires.as_mut().reset(tokio::time::Instant::from_std(result?));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::pending,
        sync::atomic::{AtomicBool, Ordering},
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
                    Duration::from_millis(1),
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
