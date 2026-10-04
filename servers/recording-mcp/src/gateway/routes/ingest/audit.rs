//! Recording ingress audit classification and private producer resource addresses.
use veoveo_mcp_contract::audit::{
    AuditDetail, AuditDraft, AuditOutcome, AuditReason, RecordingActivity,
};
use veoveo_mcp_contract::{GatewayAction, RecordingIngestStreamId, RecordingProducerId};
use veoveo_mcp_gateway::AuthenticatedSubject;

pub(crate) fn draft(
    subject: &AuthenticatedSubject,
    producer: &RecordingProducerId,
    stream: Option<&RecordingIngestStreamId>,
    action: GatewayAction,
    outcome: AuditOutcome,
    reason: AuditReason,
) -> anyhow::Result<AuditDraft> {
    let activity = match action {
        GatewayAction::RecordingStreamOpen => RecordingActivity::StreamOpen,
        GatewayAction::RecordingStreamStatus => RecordingActivity::StreamStatus,
        GatewayAction::RecordingBatchAppend => RecordingActivity::AppendDenied,
        GatewayAction::RecordingBlueprintPublish => RecordingActivity::BlueprintPublish,
        GatewayAction::RecordingStreamFinish => RecordingActivity::Finish,
        _ => anyhow::bail!("unsupported recording ingress audit action"),
    };
    let authority = veoveo_mcp_contract::audit::AuditAuthority {
        work_context: Some(subject.authority.work_context.clone()),
        policy_revision: Some(subject.authority.policy_revision.clone()),
        scopes: subject.actor.scopes.clone(),
        data_labels: subject.actor.data_labels.clone(),
        ..Default::default()
    };
    Ok(AuditDraft::builder(
        subject.audit.clone(),
        veoveo_mcp_gateway::audit::recording_ingest_audit_target(producer, stream)?,
        AuditDetail::Recording { activity },
        outcome,
        reason,
    )
    .actor(subject.audit_actor()?)
    .authority(authority)
    .build()?)
}
