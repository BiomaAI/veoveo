//! Consuming close futures stay with the original owner across interruption.
use super::*;
use anyhow::Context;
use std::{future::Future, pin::Pin, time::Instant};
type Closing = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
#[derive(Default)]
pub(super) struct CloseState {
    future: Option<Closing>,
    end: Option<Instant>,
    failed: bool,
    done: bool,
}
impl CloseState {
    pub fn closed(&self) -> bool {
        !self.failed && (self.done || self.end.is_none())
    }
    pub async fn close<H, F: Future<Output = Result<()>> + Send + 'static>(
        &mut self,
        slot: &mut Option<H>,
        close: impl FnOnce(H) -> F,
        deadline: Instant,
    ) -> Result<()> {
        ensure!(!self.failed, "Time owned close previously failed");
        if self.done || (slot.is_none() && self.future.is_none()) {
            return Ok(());
        }
        let end = *self
            .end
            .get_or_insert_with(|| (Instant::now() + Duration::from_secs(10)).min(deadline));
        // timeout_at may poll an already-ready future before observing expiry.
        // Re-entry must never turn completion after the original cap into success.
        if Instant::now() >= end {
            self.failed = true;
            anyhow::bail!("Time owned close original deadline expired");
        }
        if self.future.is_none() {
            self.future = Some(Box::pin(close(
                slot.take().context("Time close ownership missing")?,
            )));
        }
        match tokio::time::timeout_at(end.into(), self.future.as_mut().unwrap()).await {
            Ok(Ok(())) => {
                self.future = None;
                self.done = true;
            }
            _ => self.failed = true,
        }
        ensure!(
            self.done && !self.failed,
            "Time owned close unresolved under original deadline"
        );
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[tokio::test]
    async fn interrupted_close_retains_original_future_and_deadline() -> Result<()> {
        let mut state = CloseState::default();
        let mut slot = Some(());
        let (send, receive) = tokio::sync::oneshot::channel();
        let end = Instant::now() + Duration::from_secs(2);
        ensure!(
            tokio::time::timeout(
                Duration::from_millis(1),
                state.close(
                    &mut slot,
                    |_| async move {
                        receive.await?;
                        Ok(())
                    },
                    end
                )
            )
            .await
            .is_err()
        );
        ensure!(slot.is_none() && state.future.is_some() && !state.closed());
        let original = state.end;
        send.send(()).unwrap();
        state
            .close(
                &mut slot,
                |_| async { anyhow::bail!("must not redispatch close") },
                end,
            )
            .await?;
        ensure!(state.closed() && state.end == original);

        let mut late = CloseState::default();
        let mut slot = Some(());
        let (send, receive) = tokio::sync::oneshot::channel();
        let end = Instant::now() + Duration::from_millis(25);
        let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = completed.clone();
        ensure!(
            tokio::time::timeout(
                Duration::from_millis(1),
                late.close(
                    &mut slot,
                    |_| async move {
                        receive.await?;
                        observed.store(true, std::sync::atomic::Ordering::SeqCst);
                        Ok(())
                    },
                    end
                )
            )
            .await
            .is_err()
        );
        ensure!(late.future.is_some() && !late.failed && slot.is_none());
        let original = late.end;
        tokio::time::sleep_until((end + Duration::from_millis(1)).into()).await;
        send.send(()).unwrap();
        for _ in 0..2 {
            ensure!(
                late.close(
                    &mut slot,
                    |_| async { anyhow::bail!("must not replace expired close") },
                    Instant::now() + Duration::from_secs(10)
                )
                .await
                .is_err()
            );
            ensure!(late.failed && !late.closed() && !late.done && late.end == original);
            ensure!(
                !completed.load(std::sync::atomic::Ordering::SeqCst),
                "ready close was polled after its original deadline"
            );
        }
        Ok(())
    }
}
