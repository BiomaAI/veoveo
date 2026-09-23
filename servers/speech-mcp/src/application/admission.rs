use super::{DurableRequest, SpeechService};
use anyhow::Result;
use chrono::{TimeDelta, Utc};
use std::{
    collections::BTreeSet,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
};
use veoveo_mcp_contract::{
    ArtifactPlane, ArtifactTaskId, GatewayInternalIdentity, IssueArtifactReadCapabilityRequest,
    IssueArtifactWriteCapabilityRequest, PlaneCaller, PrincipalKind,
};
use veoveo_speech_contract::{MAX_SOURCE_BYTES, TranscribeRequest, validate_source};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskId, TaskOwner, TaskRetentionPin, TaskSnapshot,
};

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
                    task_id: task_id.to_string(),
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
                task_type: "transcribe".into(),
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
        let permit = self.queue.clone().try_acquire_owned()?;
        let request = serde_json::from_value(snapshot.request.clone())?;
        self.schedule(snapshot, request, permit).await
    }
}
