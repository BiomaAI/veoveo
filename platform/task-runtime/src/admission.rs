//! Domain admission composes with the exact retained, unclaimed Task.
use crate::{TaskError, TaskRuntime, TaskSnapshot, TaskStatus, types::RequestEnvelope};
use surrealdb::types::{RecordId, Value};
use veoveo_platform_store::{
    deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
    task_record_id,
};

impl TaskRuntime {
    /// Commit a domain admission while this Task is still queued and unclaimed.
    ///
    /// `body` is repository-owned SQL composed from trusted literals, without transaction
    /// delimiters. Bind all request values. `_admission_` bindings belong to this guard.
    /// The domain must guard its own state and resource exclusion inside the same body.
    /// This commit does not claim the Task or authorize a provider call. It never retries;
    /// an uncertain reply must not authorize dispatch or release retained protection.
    pub async fn commit_admission(
        &self,
        snapshot: &TaskSnapshot,
        body: &str,
        bindings: Vec<(&'static str, Value)>,
    ) -> Result<(), TaskError> {
        self.check_contribution(&snapshot.task_type)?;
        if snapshot.server != self.server() {
            return Err(TaskError::WrongServer(snapshot.task_id.to_string()));
        }
        if snapshot.status != TaskStatus::Queued
            || snapshot.lease_owner.is_some()
            || snapshot.lease_expires_at.is_some()
            || bindings
                .iter()
                .any(|(name, _)| name.starts_with("_admission_"))
        {
            return Err(TaskError::InvalidRecord(
                "Task admission requires an unclaimed queued snapshot".into(),
            ));
        }
        let owner = &snapshot.owner;
        let envelope = RequestEnvelope {
            input: snapshot.request.clone(),
            owner: owner.clone(),
            status_message: snapshot.status_message.clone(),
            ttl_ms: snapshot.ttl_ms,
            poll_interval_ms: snapshot.poll_interval_ms,
        };
        let sql = format!(
            "BEGIN TRANSACTION;
             LET $_admission_guard = (UPDATE ONLY $_admission_task SET updated_at = updated_at
               WHERE server = $_admission_server AND tenant = $_admission_tenant
                 AND owner = $_admission_owner AND work_context = $_admission_context
                 AND profile = $_admission_profile AND recovery_class = $_admission_class
                 AND task_type = $_admission_kind
                 AND status = 'queued' AND lease_owner = NONE AND lease_expires_at = NONE
                 AND cancel_requested_at = NONE AND updated_at = $_admission_updated
                 AND request = $_admission_request AND retention_pins = $_admission_pins RETURN AFTER);
             IF $_admission_guard = NONE {{ THROW 'task_admission_conflict'; }};
             {body}\nCOMMIT TRANSACTION;"
        );
        let mut query = self
            .platform_store()
            .client()
            .query(sql)
            .bind(("_admission_task", task_record_id(snapshot.task_id)))
            .bind((
                "_admission_server",
                RecordId::new("mcp_server", self.server().to_owned()),
            ))
            .bind((
                "_admission_tenant",
                deterministic_tenant_id(owner.tenant_key())?.record_id(),
            ))
            .bind((
                "_admission_owner",
                deterministic_principal_id(owner.tenant_key(), &owner.principal_key)?.record_id(),
            ))
            .bind((
                "_admission_context",
                deterministic_work_context_id(
                    owner.tenant_key(),
                    owner.authority.work_context.as_str(),
                )?
                .record_id(),
            ))
            .bind((
                "_admission_profile",
                RecordId::new("profile", owner.profile.clone()),
            ))
            .bind((
                "_admission_class",
                veoveo_platform_store::RecoveryClass::from(snapshot.recovery_class),
            ))
            .bind(("_admission_updated", snapshot.updated_at))
            .bind(("_admission_kind", snapshot.task_type.to_string()))
            .bind(("_admission_request", envelope.into_open_object()?))
            .bind((
                "_admission_pins",
                snapshot
                    .retention_pins
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ));
        for (key, value) in bindings {
            query = query.bind((key.to_owned(), value));
        }
        let mut response = query.await?;
        if let Some(error) =
            veoveo_platform_store::primary_transaction_error(response.take_errors())
        {
            if error.is_thrown() && error.message().contains("task_admission_conflict") {
                return Err(TaskError::Conflict(snapshot.task_id.to_string()));
            }
            return Err(error.into());
        }
        Ok(())
    }
}
