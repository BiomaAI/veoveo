//! Shared durable leases; provider observation never turns uncertainty into runnable work.
use crate::{
    TaskRuntime,
    runtime::task_event,
    types::{
        ClaimedTask, RecoveryClass, RequestEnvelope, TaskError, TaskSnapshot, parse_task_id,
        record_to_snapshot,
    },
};
use chrono::{TimeDelta, Utc};
use std::time::Duration;
use veoveo_platform_store::{TaskRecord, TaskStatus as StoreTaskStatus};
#[derive(Clone, Copy, PartialEq)]
enum ClaimKind {
    Execution,
    ProviderObservation,
}

impl TaskRuntime {
    pub async fn claim(
        &self,
        task_id: &str,
        lease_duration: Duration,
    ) -> Result<ClaimedTask, TaskError> {
        self.claim_kind(task_id, lease_duration, ClaimKind::Execution)
            .await
    }

    /// Acquire authority to examine a provider intent. Status, input, progress,
    /// result and cancellation stay intact; this does not authorize redispatch.
    pub async fn claim_observation(
        &self,
        task_id: &str,
        lease_duration: Duration,
    ) -> Result<ClaimedTask, TaskError> {
        self.claim_kind(task_id, lease_duration, ClaimKind::ProviderObservation)
            .await
    }

    async fn claim_kind(
        &self,
        task_id: &str,
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
            .ok_or_else(|| TaskError::NotFound(task_id.to_owned()))?;
        if snapshot.server != self.server() {
            return Err(TaskError::WrongServer(task_id.to_owned()));
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
            return Err(TaskError::LeaseHeld(task_id.to_owned()));
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
        let task = snapshot.task_id.record_id();
        let mut event_snapshot = snapshot.clone();
        if !observation {
            event_snapshot.status = StoreTaskStatus::Running;
            event_snapshot.status_message = Some("claimed for execution".to_owned());
        }
        event_snapshot.lease_owner = Some(self.worker_id().to_owned());
        event_snapshot.lease_expires_at = Some(lease_expires_at);
        event_snapshot.started_at = snapshot.started_at.or(Some(now));
        event_snapshot.updated_at = now;
        let event = task_event(
            &event_snapshot,
            if observation {
                "task.observer_claimed"
            } else {
                "task.claimed"
            },
        )?;
        let request = RequestEnvelope {
            input: snapshot.request.clone(),
            owner: snapshot.owner.clone(),
            status_message: event_snapshot.status_message.clone(),
            ttl_ms: snapshot.ttl_ms,
            poll_interval_ms: snapshot.poll_interval_ms,
        };
        let mut response = self
            .platform_store()
            .client()
            .query(
                "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = $next, request = $request, lease_owner = $worker, lease_expires_at = $lease_expires, started_at = started_at ?? $now, updated_at = $now WHERE status = $expected AND updated_at = $expected_updated_at AND (lease_expires_at = NONE OR lease_expires_at <= $now OR lease_owner = $worker) RETURN AFTER); IF $updated != NONE { CREATE outbox_event CONTENT $event RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;",
            )
            .bind(("task", task))
            .bind(("next", event_snapshot.status))
            .bind(("worker", self.worker_id().to_owned()))
            .bind(("request", request.into_open_object()?))
            .bind(("lease_expires", lease_expires_at))
            .bind(("now", now))
            .bind(("expected", snapshot.status))
            .bind(("expected_updated_at", snapshot.updated_at))
            .bind(("event", event))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(3)?;
        let snapshot = updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or_else(|| TaskError::Conflict(task_id.to_owned()))?;
        self.note_change();
        Ok(ClaimedTask {
            snapshot,
            lease_owner: self.worker_id().to_owned(),
            lease_expires_at,
        })
    }

    pub async fn renew_lease(
        &self,
        task_id: &str,
        lease_duration: Duration,
    ) -> Result<TaskSnapshot, TaskError> {
        if lease_duration.is_zero() {
            return Err(TaskError::InvalidRecord(
                "task lease duration must be greater than zero".to_owned(),
            ));
        }
        let task_id = parse_task_id(task_id)?;
        self.get(&task_id.to_string())
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        let now = Utc::now();
        let lease_expires = now
            + TimeDelta::from_std(lease_duration)
                .map_err(|_| TaskError::InvalidRecord("lease duration is too large".to_owned()))?;
        let mut response = self
            .platform_store()
            .client()
            .query(
                "UPDATE ONLY $task SET lease_expires_at = $lease_expires WHERE lease_owner = $worker AND lease_expires_at > $now AND (status IN ['running', 'waiting', 'cancel_requested'] OR (status = 'queued' AND recovery_class = 'provider_wait')) RETURN AFTER;",
            )
            .bind(("task", task_id.record_id()))
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
