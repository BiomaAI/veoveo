//! Approved indexing reads aggregate only after their attribution has been checked.
use crate::*;
use veoveo_mcp_knowledge_extension::CollectionId;

#[derive(Debug, Clone)]
pub struct IndexingRead {
    draft: AuditDraft,
    collection: CollectionId,
}
impl IndexingRead {
    pub fn new(draft: AuditDraft, collection: CollectionId) -> Result<Self, AuditValidationError> {
        let actor = draft.actor().ok_or(AuditValidationError::IndexingWindow)?;
        let target_matches = matches!(draft.target(), AuditTarget::Resource { server, .. }
            if server == collection.server());
        let detail_matches = match draft.detail() {
            AuditDetail::KnowledgeRead {
                observation: Some(observation),
                ..
            } => observation.collection == collection,
            AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            } => true,
            _ => false,
        };
        if actor.kind != AuditPrincipalKind::Service
            || actor.tenant.is_none()
            || actor.oauth_client.is_none()
            || !target_matches
            || !detail_matches
            || !matches!(
                draft.outcome(),
                AuditOutcome::Succeeded | AuditOutcome::Failed
            )
        {
            return Err(AuditValidationError::IndexingWindow);
        }
        Ok(Self { draft, collection })
    }
    pub fn draft(&self) -> &AuditDraft {
        &self.draft
    }
    pub fn collection(&self) -> &CollectionId {
        &self.collection
    }
}
