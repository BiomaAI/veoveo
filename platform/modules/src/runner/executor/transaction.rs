//! Owned transaction tasks finish cleanup even if their caller stops polling.
use super::super::preparation::{DatabaseEditorCredentials, PreparationRecord, record_id};
use super::*;
use crate::PreparationKey;
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
pub(crate) enum Operation {
    Infrastructure,
    PreparationFence(PreparationKey),
    PreparationComplete {
        key: PreparationKey,
        credentials: DatabaseEditorCredentials,
    },
    Header {
        content: Header,
        preparation: Option<PreparationKey>,
    },
    Migration {
        sql: &'static str,
        content: Applied,
        preparation: Option<PreparationKey>,
    },
    History,
    Prerequisites {
        modules: Vec<String>,
        key: PreparationKey,
    },
}
#[derive(Debug)]
pub(crate) enum Output {
    Written,
    History(Vec<Header>, Vec<Applied>),
}
#[derive(Debug)]
pub(crate) struct TransactionError {
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
// Database messages and detail values can contain bound credentials or owner data.
// Preserve structured causes and statement positions without formatting those values.
fn database_failure(phase: &str, error: &surrealdb::Error) -> RunnerError {
    let mut categories = Vec::new();
    let mut current = Some(error);
    for _ in 0..8 {
        let Some(error) = current else { break };
        let category = match error.query_details() {
            Some(surrealdb::types::QueryError::TransactionConflict) => "Query.TransactionConflict",
            Some(surrealdb::types::QueryError::NotExecuted) => "Query.NotExecuted",
            Some(surrealdb::types::QueryError::Cancelled) => "Query.Cancelled",
            Some(surrealdb::types::QueryError::TimedOut { .. }) => "Query.TimedOut",
            _ => error.kind_str(),
        };
        categories.push(category);
        current = error.cause();
    }
    failure(
        &format!(
            "{phase}: {} (database message and values redacted)",
            categories.join(" caused by ")
        ),
        None,
    )
}
fn checked_response(
    mut response: surrealdb::IndexedResults,
    phase: &str,
) -> Result<surrealdb::IndexedResults, RunnerError> {
    let mut errors: Vec<_> = response.take_errors().into_iter().collect();
    errors.sort_by_key(|(index, _)| *index);
    if errors.is_empty() {
        return Ok(response);
    }
    let count = errors.len();
    let causes: Vec<_> = errors
        .into_iter()
        .take(16)
        .map(|(index, error)| database_failure(&format!("statement {index}"), &error).to_string())
        .collect();
    Err(failure(
        &format!("{phase}: {count} failed statements; {}", causes.join("; ")),
        None,
    ))
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
pub(crate) async fn execute<C: Connection>(
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
            .map_err(|error| database_failure("cannot begin transaction", &error))?;
        let result = async {
            let preparation = match &operation {
                Operation::Header { preparation, .. }
                | Operation::Migration { preparation, .. } => preparation.as_ref(),
                _ => None,
            };
            if let Some(key) = preparation {
                let mut response = wait(
                    &mut receiver,
                    limits.operation_timeout,
                    transaction
                        .query(include_str!("../../../queries/preparation_lock.surql"))
                        .bind(("record", record_id())),
                )
                .await?
                .map_err(|error| {
                    database_failure("installation lane fence inspection failed", &error)
                })?;
                let record: Option<PreparationRecord> = response.take(0).map_err(|error| {
                    database_failure("installation lane fence response decoding failed", &error)
                })?;
                if record.is_none_or(|record| !record.matches(key) || !record.complete) {
                    return Err(failure(
                        "installation preparation changed before lane mutation",
                        None,
                    ));
                }
            }
            match &operation {
                Operation::Infrastructure => {
                    wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction.query(INFRASTRUCTURE),
                    )
                    .await?
                    .map_err(|error| database_failure("infrastructure transaction failed", &error))
                    .and_then(|response| {
                        checked_response(response, "infrastructure transaction failed")
                    })?;
                }
                Operation::PreparationFence(key) | Operation::PreparationComplete { key, .. } => {
                    let mut response = wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction
                            .query(include_str!("../../../queries/preparation_lock.surql"))
                            .bind(("record", record_id())),
                    )
                    .await?
                    .map_err(|error| {
                        database_failure("preparation fence inspection failed", &error)
                    })?;
                    let existing: Option<PreparationRecord> =
                        response.take(0).map_err(|error| {
                            database_failure("preparation fence response decoding failed", &error)
                        })?;
                    if let Some(record) = &existing {
                        record.check_advance(key)?;
                    }
                    let complete = matches!(operation, Operation::PreparationComplete { .. });
                    if complete && existing.as_ref().is_none_or(|record| !record.matches(key)) {
                        return Err(failure(
                            "preparation completion has no matching generation claim",
                            None,
                        ));
                    }
                    // An already-completed identity never rotates credentials again.
                    if !existing
                        .as_ref()
                        .is_some_and(|record| record.matches(key) && record.complete)
                    {
                        if let Operation::PreparationComplete { credentials, .. } = &operation {
                            wait(
                                &mut receiver,
                                limits.operation_timeout,
                                transaction
                                    .query(credentials.statement())
                                    .bind(("username", credentials.username().to_owned())),
                            )
                            .await?
                            .and_then(|r| r.check())
                            .map_err(|error| {
                                database_failure("database-editor rotation failed", &error)
                            })?;
                        }
                        wait(
                            &mut receiver,
                            limits.operation_timeout,
                            transaction
                                .query(include_str!("../../../queries/preparation_write.surql"))
                                .bind(("record", record_id()))
                                .bind(("content", PreparationRecord::new(key, complete))),
                        )
                        .await?
                        .and_then(|r| r.check())
                        .map_err(|error| {
                            database_failure("preparation marker write failed", &error)
                        })?;
                    }
                }
                Operation::Header { content, .. } => {
                    wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction
                            .query(include_str!("../../../queries/create_history.surql"))
                            .bind(("record", content.id.clone()))
                            .bind(("content", content.clone())),
                    )
                    .await?
                    .map_err(|error| {
                        database_failure("lane initialization transaction failed", &error)
                    })
                    .and_then(|response| {
                        checked_response(response, "lane initialization transaction failed")
                    })?;
                }
                Operation::Migration { sql, content, .. } => {
                    wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction.query(*sql),
                    )
                    .await?
                    .map_err(|error| database_failure("migration body transaction failed", &error))
                    .and_then(|response| {
                        checked_response(response, "migration body transaction failed")
                    })?;
                    wait(
                        &mut receiver,
                        limits.operation_timeout,
                        transaction
                            .query(include_str!("../../../queries/create_history.surql"))
                            .bind(("record", content.id.clone()))
                            .bind(("content", content.clone())),
                    )
                    .await?
                    .map_err(|error| {
                        database_failure("migration history transaction failed", &error)
                    })
                    .and_then(|response| {
                        checked_response(response, "migration history transaction failed")
                    })?;
                }
                Operation::History | Operation::Prerequisites { .. } => {
                    if let Operation::Prerequisites { key, .. } = &operation {
                        let mut response = wait(
                            &mut receiver,
                            limits.operation_timeout,
                            transaction
                                .query(include_str!("../../../queries/preparation_read.surql"))
                                .bind(("record", record_id())),
                        )
                        .await?
                        .map_err(|error| {
                            database_failure("runtime preparation inspection failed", &error)
                        })?;
                        let record: Option<PreparationRecord> =
                            response.take(0).map_err(|error| {
                                database_failure(
                                    "runtime preparation marker response decoding failed",
                                    &error,
                                )
                            })?;
                        let record = record.ok_or_else(|| {
                            failure("runtime installation preparation is absent", None)
                        })?;
                        record.check_advance(key)?;
                        if !record.matches(key) || !record.complete {
                            return Err(failure(
                                "runtime installation preparation is incomplete",
                                None,
                            ));
                        }
                    }
                    let query = match &operation {
                        Operation::Prerequisites { modules, .. } => transaction
                            .query(include_str!("../../../queries/prerequisite_history.surql"))
                            .bind(("modules", modules.clone())),
                        _ => transaction.query(include_str!("../../../queries/history.surql")),
                    };
                    let mut response = wait(&mut receiver, limits.operation_timeout, query)
                        .await?
                        .map_err(|error| database_failure("history transaction failed", &error))?;
                    let headers = response.take(0).map_err(|error| {
                        database_failure("lane history response decoding failed", &error)
                    })?;
                    let applied = response.take(1).map_err(|error| {
                        database_failure("migration history response decoding failed", &error)
                    })?;
                    return Ok(Output::History(headers, applied));
                }
            }
            Ok(Output::Written)
        }
        .await;
        if result.is_err()
            || matches!(
                operation,
                Operation::History | Operation::Prerequisites { .. }
            )
            || receiver.try_recv().is_ok()
        {
            tokio::time::timeout(limits.cancel_timeout, transaction.cancel()).await
                .map_err(|_| failure("transaction cancel timed out; database session cleanup required", None))?
                .map_err(|error| database_failure("transaction cancel could not be confirmed; database session cleanup required", &error))?;
            if result.is_ok()
                && !matches!(
                    operation,
                    Operation::History | Operation::Prerequisites { .. }
                )
            {
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
            .map_err(|error| TransactionError::observe(database_failure("transaction commit not confirmed; observe committed history before further action", &error)))?;
        result.map_err(TransactionError::observe)
    });
    task.await.map_err(|_| {
        failure(
            "transaction cleanup task failed; database session cleanup required",
            None,
        )
    })?
}

#[cfg(test)]
mod diagnostics_tests {
    use super::*;
    #[test]
    fn structured_causes_preserve_conflicts_without_disclosing_messages() {
        let error = surrealdb::Error::query(
            "private-token".into(),
            Some(surrealdb::types::QueryError::TransactionConflict),
        )
        .with_cause(surrealdb::Error::thrown("private-token".into()));
        let diagnostic = database_failure("migration body transaction failed", &error);
        assert!(
            diagnostic
                .to_string()
                .contains("Query.TransactionConflict caused by Thrown")
        );
        assert!(!format!("{diagnostic:?} {diagnostic}").contains("private-token"));
    }
}
