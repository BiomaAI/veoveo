//! Owned read transactions finish cancellation even when their request awaiter disappears.
use std::{future::Future, pin::Pin, time::Duration};
use surrealdb::{Connection, Surreal, method::Transaction};
use tokio::sync::oneshot;

pub(super) type ReadFuture<'a, T> = Pin<Box<dyn Future<Output = anyhow::Result<T>> + Send + 'a>>;
struct CancelOnDrop(Option<oneshot::Sender<()>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

pub(super) async fn read<T: Send + 'static, C: Connection>(
    db: &Surreal<C>,
    operation: impl for<'a> FnOnce(&'a Transaction<C>) -> ReadFuture<'a, T> + Send + 'static,
) -> anyhow::Result<T> {
    let db = db.clone();
    let (sender, mut receiver) = oneshot::channel();
    let _guard = CancelOnDrop(Some(sender));
    let worker = tokio::spawn(async move {
        // Keep begin owned after caller cancellation. An unknown begin reply has no
        // transaction handle to cancel, matching the module runner's diagnostic.
        let transaction = tokio::time::timeout(Duration::from_secs(30), db.begin())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "catalog read begin timed out; database session cleanup may be required"
                )
            })??;
        let result = tokio::select! {
            _ = &mut receiver => Err(anyhow::anyhow!("catalog read request was cancelled")),
            result = tokio::time::timeout(Duration::from_secs(30), operation(&transaction)) => {
                result.map_err(|_| anyhow::anyhow!("catalog read transaction timed out")).and_then(|result| result)
            }
        };
        let cleanup = tokio::time::timeout(Duration::from_secs(10), transaction.cancel())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "catalog read cancellation timed out; database session cleanup may be required"
                )
            })
            .and_then(|result| result.map_err(anyhow::Error::from));
        if let Err(error) = &cleanup {
            tracing::warn!("Optimization catalog read cleanup failed: {error}");
        }
        match (result, cleanup) {
            (Ok(value), Ok(_)) => Ok(value),
            (Err(error), Ok(_)) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(cleanup)) => {
                Err(error.context(format!("read transaction cancellation failed: {cleanup}")))
            }
        }
    });
    worker.await?
}
