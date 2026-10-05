//! Shared durable leases; provider observation never turns uncertainty into runnable work.
use crate::{
    TaskRuntime,
    types::{
        ClaimedTask, RecoveryClass, TaskError, TaskSnapshot, record_to_snapshot, validate_task_id,
    },
};
use chrono::{TimeDelta, Utc};
use std::time::Duration;
use surrealdb::types::SurrealValue;
use veoveo_platform_store::TaskRequestRecord;
use veoveo_platform_store::task_record_id;
use veoveo_platform_store::{TaskRecord, TaskStatus as StoreTaskStatus};
use veoveo_types::TaskId;
#[derive(Clone, Copy, PartialEq)]
enum ClaimKind {
    Execution,
    ProviderObservation,
}

impl TaskRuntime {
    /// Release only this exact observer receipt after its local provider future
    /// has ended. Another replica can continue the persisted observation schedule.
    pub async fn release_observation(&self, claimed: &ClaimedTask) -> Result<(), TaskError> {
        self.check_contribution(&claimed.snapshot.task_type)?;
        if claimed.snapshot.server != self.server()
            || claimed.snapshot.recovery_class != RecoveryClass::ProviderWait
            || claimed.lease_owner != self.worker_id()
        {
            return Err(TaskError::InvalidRecord(
                "invalid observation release".into(),
            ));
        }
        let mut response = self
            .platform_store()
            .client()
            .query(include_str!("../queries/leases/release_observation.surql"))
            .bind(("task", task_record_id(claimed.snapshot.task_id)))
            .bind((
                "server",
                surrealdb::types::RecordId::new("mcp_server", self.server().to_owned()),
            ))
            .bind(("worker", self.worker_id().to_owned()))
            .bind(("expiry", claimed.lease_expires_at))
            .await?
            .check()?;
        let row: Option<TaskRecord> = response.take(0)?;
        if row.is_none() {
            return Err(TaskError::LeaseHeld(claimed.snapshot.task_id.to_string()));
        }
        Ok(())
    }
    pub async fn claim(
        &self,
        task_id: TaskId,
        lease_duration: Duration,
    ) -> Result<ClaimedTask, TaskError> {
        self.claim_kind(task_id, lease_duration, ClaimKind::Execution)
            .await
    }

    /// Acquire authority to examine a provider intent. Status, input, progress,
    /// result and cancellation stay intact; this does not authorize redispatch.
    pub async fn claim_observation(
        &self,
        task_id: TaskId,
        lease_duration: Duration,
    ) -> Result<ClaimedTask, TaskError> {
        self.claim_kind(task_id, lease_duration, ClaimKind::ProviderObservation)
            .await
    }

    async fn claim_kind(
        &self,
        task_id: TaskId,
        lease_duration: Duration,
        kind: ClaimKind,
    ) -> Result<ClaimedTask, TaskError> {
        let observation = kind == ClaimKind::ProviderObservation;
        if lease_duration.is_zero() {
            return Err(TaskError::InvalidRecord(
                "task lease duration must be greater than zero".to_owned(),
            ));
        }
        let snapshot = self
            .get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        if snapshot.server != self.server() {
            return Err(TaskError::WrongServer(task_id.to_string()));
        }
        if (snapshot.recovery_class == RecoveryClass::ProviderWait) != observation {
            return Err(TaskError::InvalidRecord(
                "provider_wait requires claim_observation; other profiles require claim".into(),
            ));
        }
        let now = Utc::now();
        if snapshot.lease_expires_at.is_some_and(|expiry| {
            expiry > now && snapshot.lease_owner.as_deref() != Some(self.worker_id())
        }) {
            return Err(TaskError::LeaseHeld(task_id.to_string()));
        }
        if snapshot.is_terminal()
            || (!observation && snapshot.status == StoreTaskStatus::CancelRequested)
        {
            return Err(TaskError::InvalidTransition {
                from: snapshot.status,
                to: StoreTaskStatus::Running,
            });
        }
        let lease_expires_at = now
            + TimeDelta::from_std(lease_duration)
                .map_err(|_| TaskError::InvalidRecord("lease duration is too large".to_owned()))?;
        let task = task_record_id(snapshot.task_id);
        let (status, status_message) = if observation {
            (snapshot.status, snapshot.status_message.clone())
        } else {
            (StoreTaskStatus::Running, Some("Running".to_owned()))
        };

        let request = TaskRequestRecord {
            input: snapshot.request.clone(),
            status_message,
            ttl_ms: snapshot.ttl_ms,
            poll_interval_ms: snapshot.poll_interval_ms,
        };
        let mut response = self
            .platform_store()
            .client()
            .query(include_str!("../queries/leases/claim_kind.surql"))
            .bind(("task", task))
            .bind(("next", status))
            .bind(("worker", self.worker_id().to_owned()))
            .bind(("request", request.into_value()))
            .bind(("lease_expires", lease_expires_at))
            .bind(("now", now))
            .bind(("expected", snapshot.status))
            .bind(("expected_updated_at", snapshot.updated_at))
            .bind(("expected_request", TaskRequestRecord::from(&snapshot)))
            .bind((
                "expected_owner_context",
                veoveo_platform_store::TaskOwnerRecord::try_from(&snapshot.owner)?,
            ))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(2)?;
        let snapshot = updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or_else(|| TaskError::Conflict(task_id.to_string()))?;
        self.note_change();
        Ok(ClaimedTask {
            snapshot,
            lease_owner: self.worker_id().to_owned(),
            lease_expires_at,
        })
    }

    pub async fn renew_lease(
        &self,
        task_id: TaskId,
        lease_duration: Duration,
    ) -> Result<TaskSnapshot, TaskError> {
        if lease_duration.is_zero() {
            return Err(TaskError::InvalidRecord(
                "task lease duration must be greater than zero".to_owned(),
            ));
        }
        let task_id = validate_task_id(task_id)?;
        self.get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        let now = Utc::now();
        let lease_expires = now
            + TimeDelta::from_std(lease_duration)
                .map_err(|_| TaskError::InvalidRecord("lease duration is too large".to_owned()))?;
        let mut response = self
            .platform_store()
            .client()
            .query(include_str!("../queries/leases/renew_lease.surql"))
            .bind(("task", task_record_id(task_id)))
            .bind(("worker", self.worker_id().to_owned()))
            .bind(("lease_expires", lease_expires))
            .bind(("now", now))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(0)?;
        updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or_else(|| TaskError::LeaseHeld(task_id.to_string()))
    }
}
