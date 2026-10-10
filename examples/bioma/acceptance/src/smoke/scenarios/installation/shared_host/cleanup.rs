//! Actual connection, subscription and one dispatched restart futures survive owner cancellation.
use super::*;
use recovery::{Event, Journal};
use rmcp::service::Subscription;
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::sync::Mutex;
use veoveo_testing_support::{
    SmokeMcpClient,
    lifecycle::owner::{self, CleanupKind, CleanupRegistration},
};

type Closing = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
type Restart = Pin<Box<dyn Future<Output = Result<DrainReceipt>> + Send>>;
#[derive(Default)]
pub(super) struct Handles {
    pub client: Option<SmokeMcpClient>,
    pub listener: Option<Subscription>,
    pub restart: Option<Restart>,
    pub drain: Option<DrainReceipt>,
    listener_close: Option<Closing>,
    client_close: Option<Closing>,
    client_end: Option<tokio::time::Instant>,
    listener_end: Option<tokio::time::Instant>,
    end: Option<tokio::time::Instant>,
    failed: bool,
}
impl Handles {
    pub async fn close_listener(&mut self, end: tokio::time::Instant) -> Result<()> {
        if self.listener_close.is_none()
            && let Some(mut listener) = self.listener.take()
        {
            self.listener_end = Some(end.min(tokio::time::Instant::now() + Duration::from_secs(5)));
            self.listener_close = Some(Box::pin(async move {
                listener
                    .cancel()
                    .await
                    .map_err(|_| anyhow!("DuckDB subscription close failed"))
            }));
        }
        let original_end = self.listener_end.unwrap_or(end).min(end);
        finish(&mut self.listener_close, original_end, &mut self.failed).await?;
        ensure!(!self.failed, "DuckDB prior SDK cleanup failed");
        Ok(())
    }
    pub async fn close(&mut self, journal: &Journal) -> Result<()> {
        let end = match self.end {
            Some(end) => end,
            None => {
                let end = tokio::time::Instant::from_std(owner::cleanup_deadline()?);
                self.end = Some(end);
                end
            }
        };
        let intent = journal.record(Event::CleanupIntent, None, self.drain.as_ref());
        let mut restart_result = Ok(());
        if let Some(restart) = self.restart.as_mut() {
            match tokio::time::timeout_at(end, restart).await {
                Ok(Ok(receipt)) => {
                    self.drain = Some(receipt);
                    self.restart = None;
                }
                Ok(Err(_)) => {
                    self.restart = None;
                    self.failed = true;
                    restart_result = Err(anyhow!(
                        "original restart failed; physical outcome unqualified"
                    ));
                }
                Err(_) => {
                    self.failed = true;
                    restart_result =
                        Err(anyhow!("original restart unresolved at cleanup deadline"));
                }
            }
        }
        let listener = self.close_listener(end).await;
        if self.client_close.is_none()
            && let Some(client) = self.client.take()
        {
            self.client_end = Some(end.min(tokio::time::Instant::now() + Duration::from_secs(10)));
            self.client_close = Some(Box::pin(async move {
                client
                    .cancel()
                    .await
                    .map_err(|_| anyhow!("DuckDB connection close failed"))
            }));
        }
        let caller = finish(
            &mut self.client_close,
            self.client_end.unwrap_or(end).min(end),
            &mut self.failed,
        )
        .await;
        let recorded = journal.record(
            if self.failed {
                Event::CleanupUnqualified
            } else {
                Event::CleanupClosed
            },
            None,
            self.drain.as_ref(),
        );
        restart_result?;
        listener?;
        caller?;
        intent?;
        recorded?;
        ensure!(!self.failed, "DuckDB previous cleanup remains unqualified");
        Ok(())
    }
}
async fn finish(
    slot: &mut Option<Closing>,
    end: tokio::time::Instant,
    failed: &mut bool,
) -> Result<()> {
    let Some(future) = slot.as_mut() else {
        return Ok(());
    };
    match tokio::time::timeout_at(end, future).await {
        Ok(result) => {
            *slot = None;
            if result.is_err() {
                *failed = true;
            }
            result
        }
        Err(_) => {
            *failed = true;
            bail!("DuckDB cleanup deadline; original close retained")
        }
    }
}
pub(super) fn retained(
    journal: Arc<Journal>,
) -> Result<(Arc<Mutex<Handles>>, CleanupRegistration)> {
    let handles = Arc::new(Mutex::new(Handles::default()));
    let captured = handles.clone();
    let registration = owner::register_cleanup(
        CleanupKind::Remote,
        "DuckDB working-query recovery handles",
        &uuid::Uuid::now_v7().to_string(),
        move || async move {
            let dropped = journal.record(Event::OperationDropped, None, None);
            let close = captured.lock().await.close(&journal).await;
            dropped?;
            close
        },
    )?;
    Ok((handles, registration))
}
