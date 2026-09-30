use std::sync::Arc;
use veoveo_audit::AuditWriter;
use veoveo_mcp_contract::audit::{
    AuditContext, AuditDetail, AuditDraft, AuditOutcome, AuditReason, AuditTarget, LiveViewActivity,
};
use veoveo_platform_store::PlatformStore;
use veoveo_types::ResourceUri;

#[derive(Clone)]
pub(super) struct LiveViewAudit {
    writer: AuditWriter,
}
impl LiveViewAudit {
    pub(super) fn new(store: PlatformStore) -> Arc<Self> {
        let writer = AuditWriter::start(store);
        Arc::new(Self { writer })
    }
    pub(super) fn draft(
        context: &AuditContext,
        uri: ResourceUri,
        activity: LiveViewActivity,
        outcome: AuditOutcome,
        reason: AuditReason,
    ) -> anyhow::Result<AuditDraft> {
        Ok(context.draft(
            AuditTarget::Resource {
                server: veoveo_mcp_contract::ServerSlug::new("uav-sim")?,
                uri,
            },
            AuditDetail::LiveView { activity },
            outcome,
            reason,
        )?)
    }
    pub(super) async fn required(&self, draft: AuditDraft) -> anyhow::Result<()> {
        self.writer.record(draft).await?;
        Ok(())
    }
    /// Closing access takes effect immediately; the original record ID survives retries.
    pub(super) async fn completion(&self, draft: AuditDraft) {
        self.writer.record_completion(draft).await;
    }
    pub(super) fn is_running(&self) -> bool {
        self.writer.is_running()
    }
    pub(super) async fn closed(&self) {
        self.writer.closed().await;
    }
    pub(super) async fn shutdown(&self) -> anyhow::Result<()> {
        self.writer
            .shutdown(std::time::Duration::from_secs(30))
            .await?;
        Ok(())
    }
}
