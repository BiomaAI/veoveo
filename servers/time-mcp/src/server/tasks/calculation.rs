//! One CPU job retained outside cancellable async execution.
use anyhow::{Context, Result};
use rmcp::model::CallToolResult;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(super) struct Calculation {
    stop: CancellationToken,
    job: Option<JoinHandle<Result<CallToolResult>>>,
}
impl Calculation {
    pub(super) fn new(stop: &CancellationToken) -> Self {
        Self {
            stop: stop.child_token(),
            job: None,
        }
    }
    pub(super) async fn run(
        &mut self,
        work: impl FnOnce(CancellationToken) -> Result<CallToolResult> + Send + 'static,
    ) -> Result<CallToolResult> {
        let stop = self.stop.clone();
        self.job = Some(tokio::task::spawn_blocking(move || work(stop)));
        let result = self.job.as_mut().expect("owned calculation").await;
        self.job = None;
        result.context("temporal calculation worker failed")?
    }
    pub(super) async fn stop_and_drain(&mut self) {
        self.stop.cancel();
        if let Some(job) = self.job.as_mut() {
            // Keep the original handle in its slot even if this await is dropped.
            let _ = job.await;
            self.job = None;
        }
    }
}
impl Drop for Calculation {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(job) = &self.job {
            // Tokio can cancel a queued job; a running job exits at its next
            // cooperative checkpoint. Normal worker exit always awaits above.
            job.abort();
        }
    }
}

#[cfg(test)]
mod tests;
