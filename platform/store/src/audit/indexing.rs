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
/// Indexing receipts use unprefixed SHA-256 text in their frozen storage profile.
#[derive(Debug, Clone)]
struct HexDigest(Sha256Digest);
impl SurrealValue for HexDigest {
    fn kind_of() -> surrealdb::types::Kind {
        surrealdb::types::Kind::String
    }
    fn into_value(self) -> Value {
        self.0.hex().to_owned().into_value()
    }
    fn from_value(value: Value) -> Result<Self, surrealdb::types::Error> {
        let value = String::from_value(value)?;
        Sha256Digest::from_hex(value)
            .map(Self)
            .map_err(|_| surrealdb::types::Error::internal("invalid indexing digest".into()))
    }
}

#[derive(Debug, Clone, SurrealValue)]
pub(super) struct IndexingReadRecord {
    receipt: RecordId,
    fingerprint: HexDigest,
    scope: HexDigest,
    occurred_at: DateTime<Utc>,
    #[surreal(wrap)]
    collection: CollectionId,
    template: Document,
    member: Option<HexDigest>,
    genesis: HexDigest,
    not_modified: i64,
    failed: i64,
}

pub(super) fn encode_read(
    registry: &AuditTargetRegistry,
    read: &IndexingRead,
) -> Result<IndexingReadRecord, StoreError> {
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
    Ok(IndexingReadRecord {
        receipt: RecordId::new(
            "audit_indexing_receipt",
            Array::from(vec![
                draft.partition().storage_key().into_value(),
                SurrealUuid::from(draft.id().as_uuid()).into_value(),
            ]),
        ),
        fingerprint: HexDigest(hash((read.collection(), draft))),
        scope: HexDigest(hash((actor, draft.authority(), read.collection()))),
        occurred_at: draft.occurred_at(),
        collection: read.collection().clone(),
        template: Document::encode(template),
        member: member.map(HexDigest),
        genesis: HexDigest(Sha256Digest::from_bytes(
            Sha256::digest(DOMAIN.as_bytes()).into(),
        )),
        not_modified: i64::from(observation.is_some_and(|o| o.not_modified)),
        failed: i64::from(draft.outcome() == AuditOutcome::Failed),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexing_row_keeps_absent_member_native_receipt_and_dates() {
        let descriptor = veoveo_mcp_knowledge_extension::docs::collection(
            &"knowledge".parse().unwrap(),
            &"knowledge".parse().unwrap(),
        );
        let draft = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Resource {
                server: "knowledge".parse().unwrap(),
                uri: veoveo_types::ResourceUri::new("knowledge://docs/design").unwrap(),
            },
            AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            },
            AuditOutcome::Failed,
            AuditReason::UpstreamFailure,
        )
        .actor(AuditActor {
            principal: "indexer".parse().unwrap(),
            kind: AuditPrincipalKind::Service,
            tenant: Some("tenant".parse().unwrap()),
            oauth_client: Some("indexer".parse().unwrap()),
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        })
        .build()
        .unwrap();
        let read = IndexingRead::new(draft.clone(), descriptor.collection().clone()).unwrap();
        let value = encode_read(&AuditTargetRegistry::empty(), &read)
            .unwrap()
            .into_value();
        let Value::Object(mut fields) = value.clone() else {
            panic!("indexing row");
        };
        assert_eq!(fields.get("member"), Some(&Value::None));
        assert_eq!(
            fields.get("occurred_at"),
            Some(&draft.occurred_at().into_value())
        );
        assert_eq!(fields.get("failed"), Some(&1_i64.into_value()));
        assert_eq!(
            fields.get("fingerprint"),
            Some(
                &hash((read.collection(), &draft))
                    .hex()
                    .to_owned()
                    .into_value()
            )
        );
        assert!(matches!(fields.get("receipt"), Some(Value::RecordId(_))));
        assert!(
            IndexingReadRecord::from_value(value)
                .unwrap()
                .member
                .is_none()
        );
        fields.insert("receipt", "native-record-required".into_value());
        assert!(IndexingReadRecord::from_value(fields.into_value()).is_err());
    }

    #[test]
    fn indexing_digest_retains_unprefixed_storage_profile() {
        let digest = Sha256Digest::from_bytes([255; 32]);
        assert_eq!(
            HexDigest(digest.clone()).into_value(),
            digest.hex().to_owned().into_value()
        );
        assert_eq!(
            HexDigest::from_value(digest.hex().to_owned().into_value())
                .unwrap()
                .0,
            digest
        );
        assert!(HexDigest::from_value(digest.to_string().into_value()).is_err());
        assert!(
            HexDigest::from_value(RecordId::new("audit_record", "native").into_value()).is_err()
        );
    }
}
