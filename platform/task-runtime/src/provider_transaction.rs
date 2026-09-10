//! Compose a domain journal write with the current shared observation lease.
use crate::{ClaimedTask, RecoveryClass, TaskError, TaskRuntime};
use surrealdb::types::{RecordId, Value};

/// Cancellation prevents new dispatch; observation may still settle an old effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCommit {
    Dispatch,
    Observe,
}

impl TaskRuntime {
    /// Atomically commit a domain-owned journal/fence under this exact Task lease.
    ///
    /// `body` is trusted static repository SQL, without transaction delimiters. Bind
    /// all request values; `_provider_` names belong to this shared guard. A domain
    /// must compare its own dispatch stage and resource fence in the same body.
    /// Successful commit does not independently authorize a provider side effect.
    ///
    /// This call never retries. An unknown database reply cannot authorize dispatch.
    /// Lease renewal changes the receipt, requiring the caller's fresh claimed state.
    pub async fn commit_provider_journal(
        &self,
        claimed: &ClaimedTask,
        kind: ProviderCommit,
        body: &'static str,
        bindings: Vec<(&'static str, Value)>,
    ) -> Result<(), TaskError> {
        let snapshot = &claimed.snapshot;
        if snapshot.server != self.server() {
            return Err(TaskError::WrongServer(snapshot.task_id.to_string()));
        }
        if snapshot.recovery_class != RecoveryClass::ProviderWait
            || claimed.lease_owner != self.worker_id()
            || snapshot.lease_owner.as_deref() != Some(self.worker_id())
            || snapshot.lease_expires_at != Some(claimed.lease_expires_at)
            || bindings
                .iter()
                .any(|(name, _)| name.starts_with("_provider_"))
        {
            return Err(TaskError::InvalidRecord(
                "invalid provider journal lease receipt".into(),
            ));
        }
        let sql = format!(
            "BEGIN TRANSACTION; \
             LET $_provider_guard = (UPDATE ONLY $_provider_task SET lease_expires_at = lease_expires_at \
             WHERE server = $_provider_server AND recovery_class = 'provider_wait' \
             AND lease_owner = $_provider_worker AND lease_expires_at = $_provider_expiry \
             AND lease_expires_at > time::now() \
             AND status IN ['queued', 'running', 'waiting', 'cancel_requested'] \
             AND ($_provider_dispatch = false OR status != 'cancel_requested') RETURN AFTER); \
             IF $_provider_guard = NONE {{ THROW 'provider_lease_lost'; }}; \
             {body}\nCOMMIT TRANSACTION;"
        );
        let mut query = self
            .platform_store()
            .client()
            .query(sql)
            .bind(("_provider_task", snapshot.task_id.record_id()))
            .bind((
                "_provider_server",
                RecordId::new("mcp_server", self.server().to_owned()),
            ))
            .bind(("_provider_worker", self.worker_id().to_owned()))
            .bind(("_provider_expiry", claimed.lease_expires_at))
            .bind(("_provider_dispatch", kind == ProviderCommit::Dispatch));
        for (key, value) in bindings {
            query = query.bind((key.to_owned(), value));
        }
        let mut response = query.await?;
        let errors = response.take_errors();
        if errors
            .values()
            .any(|error| error.is_thrown() && error.message().contains("provider_lease_lost"))
        {
            return Err(TaskError::LeaseHeld(snapshot.task_id.to_string()));
        }
        if let Some((_, error)) = errors.into_iter().next() {
            return Err(error.into());
        }
        Ok(())
    }
}
