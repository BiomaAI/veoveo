//! Database-clock windows, durable retry receipts and transactional finalization.
use super::{codec::Document, *};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use veoveo_mcp_knowledge_extension::CollectionId;
use veoveo_types::Sha256Digest;

const DOMAIN: &str = "veoveo.ai/audit-indexing-members/v1";
fn hash(value: impl serde::Serialize) -> Sha256Digest {
    Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&value).expect("typed audit value")).into(),
    )
}
pub(super) fn encode_read(
    registry: &AuditTargetRegistry,
    read: &IndexingRead,
) -> Result<Value, StoreError> {
    let draft = read.draft();
    registry.validate(draft.target())?;
    let actor = draft.actor().expect("checked indexing actor");
    let template = AuditDraft::builder(
        AuditRequest::background(),
        AuditTarget::Server {
            server: read.collection().server().clone(),
        },
        AuditDetail::Read {
            method: AuditReadMethod::ResourceRead,
        },
        AuditOutcome::Succeeded,
        AuditReason::Accepted,
    )
    .actor(actor.clone())
    .authority(draft.authority().clone())
    .build()?;
    let observation = match draft.detail() {
        AuditDetail::KnowledgeRead { observation, .. } => observation.as_deref(),
        _ => None,
    };
    let member = match (draft.target(), observation) {
        (AuditTarget::Resource { uri, .. }, Some(observation)) => {
            Some(hash((uri, &observation.revision)))
        }
        _ => None,
    };
    let mut row = Object::new();
    row.insert(
        "receipt",
        RecordId::new(
            "audit_indexing_receipt",
            Array::from(vec![
                draft.partition().storage_key().into_value(),
                SurrealUuid::from(draft.id().as_uuid()).into_value(),
            ]),
        )
        .into_value(),
    );
    row.insert(
        "fingerprint",
        hash((read.collection(), draft))
            .hex()
            .to_owned()
            .into_value(),
    );
    row.insert(
        "scope",
        hash((actor, draft.authority(), read.collection()))
            .hex()
            .to_owned()
            .into_value(),
    );
    row.insert("occurred_at", draft.occurred_at().into_value());
    row.insert("collection", read.collection().to_string().into_value());
    row.insert("template", Document::encode(template).into_value());
    row.insert(
        "member",
        member.map(|value| value.hex().to_owned()).into_value(),
    );
    row.insert(
        "genesis",
        Sha256Digest::from_bytes(Sha256::digest(DOMAIN.as_bytes()).into())
            .hex()
            .to_owned()
            .into_value(),
    );
    row.insert(
        "not_modified",
        i64::from(observation.is_some_and(|o| o.not_modified)).into_value(),
    );
    row.insert(
        "failed",
        i64::from(draft.outcome() == AuditOutcome::Failed).into_value(),
    );
    Ok(row.into_value())
}

#[derive(SurrealValue)]
struct Window {
    id: RecordId,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    collection: String,
    template: Document,
    reads: u64,
    not_modified: u64,
    failed: u64,
    members_digest: String,
}
impl PlatformStore {
    /// Finalize at most 32 elapsed windows. Racing replicas compare the monotonic
    /// read count and atomically append the immutable record and remove staging.
    pub async fn close_audit_indexing_windows(&self) -> Result<usize, StoreError> {
        let mut response = self
            .db
            .query(include_str!("../queries/audit/indexing_due.surql"))
            .await?
            .check()?;
        let windows: Vec<Window> = response.take(0)?;
        let count = windows.len();
        for window in windows {
            let collection: CollectionId = window
                .collection
                .parse()
                .map_err(|_| StoreError::AuditIntegrity)?;
            let template = window.template.checked(self.audit_targets())?;
            let draft = AuditDraft::builder(
                template.request().clone(),
                template.target().clone(),
                AuditDetail::IndexingWindow {
                    collection,
                    start: window.start,
                    end: window.end,
                    reads: window.reads,
                    not_modified: window.not_modified,
                    failed: window.failed,
                    members_digest: Sha256Digest::from_hex(&window.members_digest)
                        .map_err(|_| StoreError::AuditIntegrity)?,
                },
                AuditOutcome::Succeeded,
                AuditReason::Accepted,
            )
            .identity(template.id())
            .occurred_at(window.end)
            .actor(template.actor().ok_or(StoreError::AuditIntegrity)?.clone())
            .authority(template.authority().clone())
            .build()?;
            let rows = vec![encode(self.audit_targets(), draft)?];
            for attempt in 0..8 {
                let result = self
                    .db
                    .query(include_str!("../queries/audit/indexing_close.surql"))
                    .bind(("window", window.id.clone()))
                    .bind(("reads", window.reads))
                    .bind(("audit_rows", rows.clone()))
                    .await
                    .and_then(|mut response| {
                        match crate::primary_transaction_error(response.take_errors()) {
                            Some(error) => Err(error),
                            None => Ok(response),
                        }
                    });
                match result {
                    Ok(_) => break,
                    Err(error)
                        if attempt < 7
                            && matches!(
                                error.query_details(),
                                Some(surrealdb::types::QueryError::TransactionConflict)
                            ) =>
                    {
                        tokio::time::sleep(std::time::Duration::from_millis(2u64.pow(attempt)))
                            .await;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(count)
    }
    pub async fn prune_audit_indexing_receipts(&self) -> Result<usize, StoreError> {
        let mut attempt = 0;
        let mut response = loop {
            let result = self
                .db
                .query(include_str!("../queries/audit/indexing_prune.surql"))
                .await
                .and_then(|mut response| {
                    match crate::primary_transaction_error(response.take_errors()) {
                        Some(error) => Err(error),
                        None => Ok(response),
                    }
                });
            match result {
                Ok(response) => break response,
                Err(error)
                    if attempt < 7
                        && matches!(
                            error.query_details(),
                            Some(surrealdb::types::QueryError::TransactionConflict)
                        ) =>
                {
                    tokio::time::sleep(std::time::Duration::from_millis(2u64.pow(attempt))).await;
                    attempt += 1;
                }
                Err(error) => return Err(error.into()),
            }
        };
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::AuditIntegrity)?;
        let count: Option<u64> = response.take(index)?;
        usize::try_from(count.ok_or(StoreError::AuditIntegrity)?)
            .map_err(|_| StoreError::AuditIntegrity)
    }
}
