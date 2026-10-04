//! Recording ingress audit classification and private producer resource addresses.
use crate::contract::{RecordingAction, RecordingTarget, target_audit_resource};
use veoveo_mcp_contract::audit::{
    AuditDetail, AuditDraft, AuditOutcome, AuditReason, RecordingActivity,
};
use veoveo_recording_contract::RecordingIngestStreamId;
use veoveo_recording_contract::RecordingProducerId;

use veoveo_mcp_gateway::AuthenticatedSubject;

pub(crate) fn draft(
    subject: &AuthenticatedSubject,
    producer: &RecordingProducerId,
    stream: Option<&RecordingIngestStreamId>,
    action: RecordingAction,
    outcome: AuditOutcome,
    reason: AuditReason,
) -> anyhow::Result<AuditDraft> {
    let activity = match action {
        RecordingAction::StreamOpen => RecordingActivity::StreamOpen,
        RecordingAction::StreamStatus => RecordingActivity::StreamStatus,
        RecordingAction::BatchAppend => RecordingActivity::AppendDenied,
        RecordingAction::BlueprintPublish => RecordingActivity::BlueprintPublish,
        RecordingAction::StreamFinish => RecordingActivity::Finish,
        _ => anyhow::bail!("unsupported recording ingress audit action"),
    };
    let authority = veoveo_mcp_contract::audit::AuditAuthority {
        work_context: Some(subject.authority.work_context.clone()),
        policy_revision: Some(subject.authority.policy_revision.clone()),
        scopes: subject.actor.scopes.clone(),
        data_labels: subject.actor.data_labels.clone(),
        ..Default::default()
    };
    let target = match stream {
        Some(stream) => RecordingTarget::RecordingStream {
            producer: producer.clone(),
            stream_id: stream.clone(),
        },
        None => RecordingTarget::RecordingProducer {
            producer: producer.clone(),
        },
    };
    let resource = target_audit_resource(&target)?;
    Ok(AuditDraft::builder(
        subject.audit.clone(),
        veoveo_mcp_contract::audit::AuditTarget::Resource {
            server: resource.server,
            uri: resource.uri,
        },
        AuditDetail::Recording { activity },
        outcome,
        reason,
    )
    .actor(subject.audit_actor()?)
    .authority(authority)
    .build()?)
}
