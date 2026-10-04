//! Session counts and lifecycle attribution; never records audio or transcripts.
use anyhow::Result;
use veoveo_audit::AuditWriter;
use veoveo_mcp_contract::audit::{
    AuditContext, AuditDetail, AuditOutcome, AuditReason, AuditTarget, DictationEnd,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, ServerSlug};
use veoveo_speech_contract::{DictationSessionId, DictationUri};

pub(super) struct SessionAudit {
    pub context: AuditContext,
    pub samples: u64,
    pub chunks: u64,
    pub rate: u32,
    id: DictationSessionId,
    writer: AuditWriter,
}
fn target(id: DictationSessionId) -> AuditTarget {
    AuditTarget::Resource {
        server: ServerSlug::parse("speech").expect("declared Speech server"),
        uri: DictationUri::new(id).to_uri(),
    }
}
impl SessionAudit {
    pub async fn open(
        writer: AuditWriter,
        caller: &GatewayInternalIdentity,
        id: DictationSessionId,
        rate: u32,
    ) -> Result<Self> {
        let context = caller.audit_context()?;
        let draft = context.draft(
            target(id),
            AuditDetail::DictationOpen,
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )?;
        writer.record(draft).await?;
        Ok(Self {
            writer,
            context,
            id,
            rate,
            samples: 0,
            chunks: 0,
        })
    }
    pub async fn finish(self, end: DictationEnd) {
        let (outcome, reason) = match end {
            DictationEnd::Completed | DictationEnd::Cancelled => {
                (AuditOutcome::Succeeded, AuditReason::Accepted)
            }
            DictationEnd::TimedOut => (AuditOutcome::Failed, AuditReason::TimedOut),
            DictationEnd::Disconnected => (AuditOutcome::Failed, AuditReason::Cancelled),
            DictationEnd::Failed => (AuditOutcome::Failed, AuditReason::UpstreamFailure),
        };
        let draft = self
            .context
            .draft(
                target(self.id),
                AuditDetail::DictationSummary {
                    chunks: self.chunks,
                    duration_ms: self.samples * 1000 / u64::from(self.rate),
                    end,
                },
                outcome,
                reason,
            )
            .expect("admitted dictation has bounded duration and counts");
        self.writer.record_completion(draft).await;
    }
}

pub(super) async fn denial(
    writer: &AuditWriter,
    caller: &GatewayInternalIdentity,
    id: DictationSessionId,
    reason: AuditReason,
) -> Result<()> {
    writer
        .record(caller.audit_context()?.draft(
            target(id),
            AuditDetail::DictationDenial,
            AuditOutcome::Denied,
            reason,
        )?)
        .await?;
    Ok(())
}
