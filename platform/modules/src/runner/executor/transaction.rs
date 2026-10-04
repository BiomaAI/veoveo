//! Owned transaction tasks finish cleanup even if their caller stops polling.
use super::*;
use std::{future::IntoFuture, time::Duration};
use tokio::sync::oneshot;

#[derive(Clone, Copy, Debug)]
pub struct ExecutionLimits {
    pub operation_timeout: Duration,
    pub cancel_timeout: Duration,
}
impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            operation_timeout: Duration::from_secs(30),
            cancel_timeout: Duration::from_secs(10),
        }
    }
}
impl ExecutionLimits {
    pub(super) fn check(self) -> Result<Self, RunnerError> {
        if self.operation_timeout.is_zero()
            || self.cancel_timeout.is_zero()
            || self.operation_timeout > Duration::from_secs(300)
            || self.cancel_timeout > Duration::from_secs(60)
        {
            return Err(failure(
                "execution timeouts must be nonzero and at most 300 seconds (operation) / 60 seconds (cancel)",
                None,
            ));
        }
        Ok(self)
    }
}
struct CancelOnDrop(Option<oneshot::Sender<()>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}
pub(super) enum Operation {
    Infrastructure,
    Header(Header),
    Migration { sql: &'static str, content: Applied },
    History,
}
#[derive(Debug)]
pub(super) enum Output {
    Written,
    History(Vec<Header>, Vec<Applied>),
}
#[derive(Debug)]
pub(super) struct TransactionError {
    pub error: RunnerError,
    pub may_observe_winner: bool,
}
impl From<RunnerError> for TransactionError {
    fn from(error: RunnerError) -> Self {
        Self {
            error,
            may_observe_winner: false,
        }
    }
}
impl From<TransactionError> for RunnerError {
    fn from(error: TransactionError) -> Self {
        error.error
    }
}
impl TransactionError {
    fn observe(error: RunnerError) -> Self {
        Self {
            error,
            may_observe_winner: true,
        }
    }
}
async fn wait<F: IntoFuture>(
    cancel: &mut oneshot::Receiver<()>,
    limit: Duration,
    future: F,
) -> Result<F::Output, RunnerError> {
    tokio::select! {
        _ = cancel => Err(failure("transaction caller cancelled; mutation outcome requires history observation", None)),
        result = tokio::time::timeout(limit, future.into_future()) => result.map_err(|_| failure("transaction operation timed out; mutation outcome requires history observation", None)),
    }
}
pub(super) async fn execute<C: Connection>(
    db: &Surreal<C>,
    operation: Operation,
    limits: ExecutionLimits,
) -> Result<Output, TransactionError> {
    let limits = limits.check()?;
    let db = db.clone();
    let (sender, mut receiver) = oneshot::channel();
    let _guard = CancelOnDrop(Some(sender));
    let task = tokio::spawn(async move {
        // Keep begin owned when the caller disappears. A lost begin response has no
        // addressable handle; report uncertainty instead of inventing rollback.
        let transaction = tokio::time::timeout(limits.operation_timeout, db.begin())
            .await
            .map_err(|_| {
                failure(
                    "transaction begin timed out; database session cleanup may be required",
                    None,
                )
            })?
            .map_err(|_| failure("cannot begin transaction", None))?;
        let result = async {
            match &operation {
                Operation::Infrastructure => {
                    wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction.query(INFRASTRUCTURE),
                    )
                    .await?
                    .and_then(|response| response.check())
                    .map_err(|_| failure("infrastructure transaction failed", None))?;
                }
                Operation::Header(content) => {
                    wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction
                            .query(include_str!("../../../queries/create_history.surql"))
                            .bind(("record", content.id.clone()))
                            .bind(("content", content.clone())),
                    )
                    .await?
                    .and_then(|response| response.check())
                    .map_err(|_| failure("lane initialization transaction failed", None))?;
                }
                Operation::Migration { sql, content } => {
                    wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction.query(*sql),
                    )
                    .await?
                    .and_then(|response| response.check())
                    .map_err(|_| failure("migration body transaction failed", None))?;
                    wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction
                            .query(include_str!("../../../queries/create_history.surql"))
                            .bind(("record", content.id.clone()))
                            .bind(("content", content.clone())),
                    )
                    .await?
                    .and_then(|response| response.check())
                    .map_err(|_| failure("migration history transaction failed", None))?;
                }
                Operation::History => {
                    let mut response = wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction.query(include_str!("../../../queries/history.surql")),
                    )
                    .await?
                    .map_err(|_| failure("history transaction failed", None))?;
                    let headers = response
                        .take(0)
                        .map_err(|_| failure("invalid lane history", None))?;
                    let applied = response
                        .take(1)
                        .map_err(|_| failure("invalid migration history", None))?;
                    return Ok(Output::History(headers, applied));
                }
            }
            Ok(Output::Written)
        }
        .await;
        if result.is_err() || matches!(operation, Operation::History) || receiver.try_recv().is_ok()
        {
            tokio::time::timeout(limits.cancel_timeout, transaction.cancel()).await
                .map_err(|_| failure("transaction cancel timed out; database session cleanup required", None))?
                .map_err(|_| failure("transaction cancel could not be confirmed; database session cleanup required", None))?;
            if result.is_ok() && !matches!(operation, Operation::History) {
                return Err(TransactionError::observe(failure(
                    "transaction cancelled before commit",
                    None,
                )));
            }
            return result.map_err(TransactionError::observe);
        }
        // Commit consumes the native handle. Once dispatched, caller cancellation
        // cannot pretend to revoke it; let this owned task settle or report uncertainty.
        tokio::time::timeout(limits.operation_timeout, transaction.commit()).await
            .map_err(|_| TransactionError::observe(failure("transaction commit timed out; observe committed history before further action", None)))?
            .map_err(|_| TransactionError::observe(failure("transaction commit not confirmed; observe committed history before further action", None)))?;
        result.map_err(TransactionError::observe)
    });
    task.await.map_err(|_| {
        failure(
            "transaction cleanup task failed; database session cleanup required",
            None,
        )
    })?
}
