use super::{DurableRequest, SpeechService};
use anyhow::Result;
use chrono::{TimeDelta, Utc};
use std::{
    collections::BTreeSet,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
};
use veoveo_artifact_contract::{
    ArtifactTaskId, IssueArtifactReadCapabilityRequest, IssueArtifactWriteCapabilityRequest,
};
use veoveo_mcp_contract::{ArtifactPlane, GatewayInternalIdentity, PlaneCaller, PrincipalKind};
use veoveo_speech_contract::{MAX_SOURCE_BYTES, TranscribeRequest, validate_source};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskError, TaskOwner, TaskRetentionPin, TaskSnapshot, TaskStatus,
    TaskTransition,
};
use veoveo_types::TaskId;
use veoveo_types::TaskTypeDefinition;

pub fn owner(identity: &GatewayInternalIdentity) -> TaskOwner {
    TaskOwner {
        principal_key: identity.actor.id.to_string(),
        principal_kind: match identity.actor.kind {
            PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            PrincipalKind::Service => veoveo_task_runtime::PrincipalKind::Service,
        },
        issuer: identity.actor.issuer.to_string(),
        subject: identity.actor.subject.to_string(),
        profile: identity.profile.to_string(),
        tenant_key: identity.actor.tenant.as_ref().map(ToString::to_string),
        data_labels: identity
            .actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        authority: identity.authority.clone(),
    }
}

impl SpeechService {
    pub async fn transcribe(
        self: &Arc<Self>,
        caller: &PlaneCaller,
        input: TranscribeRequest,
        retention_pins: BTreeSet<TaskRetentionPin>,
    ) -> Result<TaskSnapshot> {
        let permit = self.queue.clone().try_acquire_owned()?;
        let source_id = input.source()?;
        let source = self.artifacts.head(caller, &source_id).await?;
        validate_source(&source)?;
        let task_id = TaskId::new();
        let read = self
            .artifacts
            .issue_read_capability(
                caller,
                &IssueArtifactReadCapabilityRequest {
                    task_id: ArtifactTaskId::parse(task_id.to_string())?,
                    expires_at: Utc::now() + TimeDelta::hours(24),
                    max_artifact_count: NonZeroU32::new(1).unwrap(),
                    max_total_bytes: NonZeroU64::new(MAX_SOURCE_BYTES).unwrap(),
                },
            )
            .await?;
        let write = self
            .artifacts
            .issue_write_capability(
                caller,
                &IssueArtifactWriteCapabilityRequest {
                    task_id: veoveo_artifact_contract::ArtifactTaskId::try_from(task_id.as_uuid())?,
                    required_data_labels: source.compliance.data_labels.clone(),
                    expires_at: Utc::now() + TimeDelta::hours(24),
                    max_artifact_count: NonZeroU32::new(2).unwrap(),
                    max_total_bytes: NonZeroU64::new(12 * 1024 * 1024).unwrap(),
                },
            )
            .await?;
        let request = DurableRequest {
            input,
            source,
            read,
            write,
        };
        let created = self
            .tasks
            .create(CreateTask {
                task_id,
                owner: owner(&caller.identity),
                server: "speech".into(),
                task_type: veoveo_speech_contract::SpeechTaskKind::Transcribe.name(),
                request: serde_json::to_value(&request)?,
                recovery_class: RecoveryClass::Resume,
                idempotency_key: None,
                ttl_ms: Some(7 * 24 * 60 * 60 * 1000),
                poll_interval_ms: Some(5000),
                retention_pins,
            })
            .await?;
        self.schedule(created.snapshot, request, permit).await
    }

    pub async fn resume(self: &Arc<Self>, snapshot: TaskSnapshot) -> Result<TaskSnapshot> {
        let request: DurableRequest = serde_json::from_value(snapshot.request.clone())?;
        validate_source(&request.source)?;
        request.input.source()?;
        let permit = recovery_permit(self.queue.clone()).await?;
        // Capacity can become available after a client cancels or another replica
        // settles the Task. Re-read durable state before claiming or dispatching.
        let current = self
            .tasks
            .get_for_recovery(snapshot.task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(snapshot.task_id.to_string()))?;
        validate_recovery_snapshot(&snapshot, &current)?;
        if current.is_terminal() {
            return Ok(current);
        }
        if current.status == TaskStatus::CancelRequested {
            return Ok(self
                .tasks
                .transition_if_current(&current, TaskTransition::Cancelled)
                .await?);
        }
        self.schedule(current, request, permit).await
    }
}

async fn recovery_permit(
    queue: Arc<tokio::sync::Semaphore>,
) -> Result<tokio::sync::OwnedSemaphorePermit> {
    Ok(queue.acquire_owned().await?)
}

pub(crate) fn validate_recovery_snapshot(
    admitted: &TaskSnapshot,
    current: &TaskSnapshot,
) -> Result<()> {
    anyhow::ensure!(
        admitted.task_id == current.task_id
            && admitted.server == current.server
            && admitted.task_type == current.task_type
            && admitted.recovery_class == current.recovery_class
            && admitted.owner == current.owner
            && admitted.request == current.request,
        "recovered Speech Task changed after admission"
    );
    Ok(())
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{Mutex, Notify, Semaphore};
    use veoveo_task_runtime::{RecoveryReport, TaskRecoveryObserver};

    #[tokio::test]
    async fn retained_backlog_exceeding_queue_capacity_keeps_serving_and_eventually_admits()
    -> Result<()> {
        let queue = Arc::new(Semaphore::new(64));
        let permits = Arc::new(Mutex::new(Vec::new()));
        let admitted = Arc::new(AtomicUsize::new(0));
        let progress = Arc::new(Notify::new());
        let stream = futures::stream::once(async { Ok(RecoveryReport::default()) });
        let observer = TaskRecoveryObserver::start_deferred(Box::pin(stream), {
            let queue = queue.clone();
            let permits = permits.clone();
            let admitted = admitted.clone();
            let progress = progress.clone();
            move |_| {
                let queue = queue.clone();
                let permits = permits.clone();
                let admitted = admitted.clone();
                let progress = progress.clone();
                async move {
                    for _ in 0..65 {
                        let permit = recovery_permit(queue.clone()).await?;
                        permits.lock().await.push(permit);
                        admitted.fetch_add(1, Ordering::SeqCst);
                        progress.notify_one();
                    }
                    Ok(())
                }
            }
        });
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            observer.serve(async {
                while admitted.load(Ordering::SeqCst) < 64 {
                    progress.notified().await;
                }
                assert_eq!(admitted.load(Ordering::SeqCst), 64);
                // Public admission still rejects immediately while the queue is full.
                assert!(queue.clone().try_acquire_owned().is_err());
                // HTTP can process a completion/cancellation while recovery awaits capacity.
                drop(permits.lock().await.pop());
                while admitted.load(Ordering::SeqCst) < 65 {
                    progress.notified().await;
                }
                assert_eq!(queue.available_permits(), 0);
                Ok(())
            }),
        )
        .await??;
        drop(permits);
        assert_eq!(queue.available_permits(), 64);
        Ok(())
    }
}
