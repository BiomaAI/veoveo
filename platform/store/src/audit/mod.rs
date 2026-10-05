//! Shared persistence for the unified audit model; policy admission precedes SQL decoding.
use crate::{PlatformClient, PlatformStore, RecordId, StoreError};
use chrono::Utc;
use surrealdb::{
    method::Query,
    types::{Array, SurrealValue, Uuid as SurrealUuid, Value},
};
pub use veoveo_audit_contract::AuditTargetRegistry;
use veoveo_audit_contract::*;
mod codec;
mod delivery;
mod indexing;
mod maintenance;
pub use codec::AuditContextRecord;
use codec::{DetailLookup, Document, NativeText, Row, TargetLookup, scalar};
const APPEND: &str = include_str!("../queries/audit/append.surql");
const LIST: &str = include_str!("../queries/audit/list.surql");

pub fn record_id(partition: &AuditPartition, id: AuditRecordId) -> RecordId {
    RecordId::new(
        "audit_record",
        Array::from(vec![
            (partition.storage_key()).into_value(),
            SurrealUuid::from(id.as_uuid()).into_value(),
        ]),
    )
}
fn target_reference(target: &AuditTarget) -> Result<Option<RecordId>, StoreError> {
    Ok(match target {
        AuditTarget::Artifact { artifact } => Some(RecordId::new(
            "artifact_occurrence",
            SurrealUuid::from(artifact.as_uuid()),
        )),
        AuditTarget::Extension(target) => {
            target
                .lookup_reference()
                .map(|reference| match reference.key() {
                    AuditLookupKey::Uuid(id) => {
                        RecordId::new(reference.table(), SurrealUuid::from(*id))
                    }
                    AuditLookupKey::Text(key) => RecordId::new(reference.table(), key.clone()),
                })
        }
        AuditTarget::Task { task } => {
            Some(RecordId::new("task", SurrealUuid::from(task.as_uuid())))
        }
        AuditTarget::TaskRoute { route, .. } => {
            Some(RecordId::new("gateway_task_route", route.as_str()))
        }
        AuditTarget::Principal { tenant, principal } => Some(
            crate::deterministic_principal_id(tenant.as_str(), principal.as_str())?.record_id(),
        ),
        AuditTarget::WorkContext { tenant, context } => Some(
            crate::deterministic_work_context_id(tenant.as_str(), context.as_str())?.record_id(),
        ),
        _ => None,
    })
}
#[derive(Debug, Clone, SurrealValue)]
struct AuditAppendRecord {
    id: RecordId,
    partition: String,
    record_id: SurrealUuid,
    #[surreal(wrap)]
    class: AuditClass,
    activity: String,
    #[surreal(wrap)]
    outcome: AuditOutcome,
    actor_key: Option<NativeText<veoveo_types::PrincipalId>>,
    target_ref: Option<RecordId>,
    profile_lookup: Option<NativeText<veoveo_types::GatewayProfileId>>,
    target_lookup: TargetLookup,
    detail_lookup: DetailLookup,
    #[surreal(wrap)]
    trace_id: AuditTraceId,
    request_id: SurrealUuid,
    occurred_at: chrono::DateTime<Utc>,
    draft: Document,
}

#[derive(Debug, Clone, SurrealValue)]
struct AuditAppendWrite {
    record: AuditAppendRecord,
    window: Option<RecordId>,
    window_start: Option<chrono::DateTime<Utc>>,
}

fn encode(
    registry: &AuditTargetRegistry,
    draft: AuditDraft,
) -> Result<AuditAppendWrite, StoreError> {
    registry.validate(draft.target())?;
    let row = AuditAppendRecord {
        id: record_id(draft.partition(), draft.id()),
        partition: draft.partition().storage_key(),
        record_id: SurrealUuid::from(draft.id().as_uuid()),
        class: draft.detail().class(),
        activity: draft.detail().activity().to_owned(),
        outcome: draft.outcome(),
        actor_key: draft.actor().map(|a| NativeText(a.principal.clone())),
        target_ref: target_reference(draft.target())?,
        profile_lookup: draft.authority().profile.clone().map(NativeText),
        target_lookup: TargetLookup::new(draft.target()),
        detail_lookup: DetailLookup(draft.detail().clone()),
        trace_id: draft.request().trace_id.clone(),
        request_id: SurrealUuid::from(draft.request().id.as_uuid()),
        occurred_at: draft.occurred_at(),
        draft: Document::encode(draft.clone()),
    };
    let window = match (
        draft.actor(),
        draft.target(),
        draft.detail(),
        draft.outcome(),
    ) {
        (
            Some(actor),
            AuditTarget::Artifact { artifact },
            AuditDetail::Artifact {
                activity: ArtifactActivity::Download,
                window_start: Some(start),
                ..
            },
            AuditOutcome::Allowed,
        ) => Some((
            RecordId::new(
                "audit_download_window",
                Array::from(vec![
                    (draft.partition().storage_key()).into_value(),
                    (actor.principal.to_string()).into_value(),
                    SurrealUuid::from(artifact.as_uuid()).into_value(),
                    (start.timestamp()).into_value(),
                ]),
            ),
            *start,
        )),
        _ => None,
    };
    Ok(AuditAppendWrite {
        record: row,
        window: window.as_ref().map(|(id, _)| id.clone()),
        window_start: window.map(|(_, start)| start),
    })
}

/// Append this statement inside an existing domain transaction before COMMIT.
/// The caller controls transaction lifetime; this value starts no independent write.
pub struct AuditTransactionWrite {
    rows: Vec<AuditAppendWrite>,
}
impl AuditTransactionWrite {
    pub fn new(registry: &AuditTargetRegistry, draft: AuditDraft) -> Result<Self, StoreError> {
        Self::batch(registry, vec![draft])
    }
    pub fn batch(
        registry: &AuditTargetRegistry,
        drafts: Vec<AuditDraft>,
    ) -> Result<Self, StoreError> {
        if drafts.len() > 4096 {
            return Err(StoreError::AuditBatchLimit);
        }
        Ok(Self {
            rows: drafts
                .into_iter()
                .map(|draft| encode(registry, draft))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
    pub fn append<'q>(self, query: Query<'q, PlatformClient>) -> Query<'q, PlatformClient> {
        query.query(APPEND).bind(("audit_rows", self.rows))
    }
    /// Bind checked records for a domain SQL branch which calls
    /// `fn::append_audit($audit_rows)` inside its existing transaction.
    pub fn into_binding(self) -> (&'static str, Value) {
        ("audit_rows", self.rows.into_value())
    }
}
impl PlatformStore {
    pub async fn append_audit_records(&self, records: &[AuditDraft]) -> Result<(), StoreError> {
        self.append_audit_group(records, &[]).await
    }
    pub async fn append_audit_group(
        &self,
        records: &[AuditDraft],
        indexing: &[IndexingRead],
    ) -> Result<(), StoreError> {
        if records.is_empty() && indexing.is_empty() {
            return Ok(());
        }
        if records.len() + indexing.len() > 64 {
            return Err(StoreError::AuditBatchLimit);
        }
        let rows = records
            .iter()
            .cloned()
            .map(|draft| encode(self.audit_targets(), draft))
            .collect::<Result<Vec<_>, _>>()?;
        let mut last = None;
        let indexing = indexing
            .iter()
            .map(|read| indexing::encode_read(self.audit_targets(), read))
            .collect::<Result<Vec<_>, _>>()?;
        for attempt in 0..4 {
            let result = self
                .db
                .query(include_str!(
                    "../queries/audit/mod/append_audit_group.surql"
                ))
                .query(APPEND)
                .query(include_str!(
                    "../queries/audit/mod/append_audit_group_indexing.surql"
                ))
                .query(include_str!(
                    "../queries/audit/mod/append_audit_group_2.surql"
                ))
                .bind(("audit_rows", rows.clone()))
                .bind(("indexing", indexing.clone()))
                .await
                .and_then(|mut response| {
                    match crate::primary_transaction_error(response.take_errors()) {
                        Some(error) => Err(error),
                        None => Ok(response),
                    }
                });
            match result {
                Ok(_) => return Ok(()),
                Err(error)
                    if matches!(
                        error.query_details(),
                        Some(surrealdb::types::QueryError::TransactionConflict)
                    ) =>
                {
                    last = Some(error);
                    tokio::time::sleep(std::time::Duration::from_millis(2u64.pow(attempt))).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(last.expect("retry has a conflict").into())
    }
    pub async fn audit_page(
        &self,
        scope: &AuditReadScope,
        query: &AuditQuery,
    ) -> Result<AuditPage, StoreError> {
        query.validate()?;
        if let Some(target) = &query.target {
            self.audit_targets().validate(target)?;
        }
        if !scope.permits(&query.partition) {
            return Err(StoreError::AuditAccessDenied);
        }
        let mut response = self
            .db
            .query(match query.order {
                AuditOrder::OldestFirst => LIST,
                AuditOrder::NewestFirst => include_str!("../queries/audit/list_descending.surql"),
            })
            .bind(("partition", query.partition.storage_key()))
            .bind((
                "after",
                query
                    .cursor
                    .as_ref()
                    .map(|c| record_id(&c.partition, c.last_id)),
            ))
            .bind(("class", query.class.map(|c| scalar(&c))))
            .bind(("actor", query.actor.as_ref().map(ToString::to_string)))
            .bind((
                "target",
                query.target.as_ref().map(codec::TargetLookup::new),
            ))
            .bind(("outcome", query.outcome.map(|c| scalar(&c))))
            .bind(("trace", query.trace.as_ref().map(ToString::to_string)))
            .bind(("from", query.from))
            .bind(("until", query.until))
            .bind(("limit", u32::from(query.limit) + 1))
            .await?
            .check()?;
        let mut rows = response
            .take::<Vec<Row>>(0)?
            .into_iter()
            .map(|row| row.checked(self.audit_targets()))
            .collect::<Result<Vec<_>, _>>()?;
        let more = rows.len() > usize::from(query.limit);
        rows.truncate(usize::from(query.limit));
        let records = rows;
        let next = more
            .then(|| {
                records.last().map(|row| AuditCursor {
                    order: query.order,
                    partition: query.partition.clone(),
                    last_id: row.draft.id(),
                })
            })
            .flatten();
        Ok(AuditPage { records, next })
    }
    pub async fn audit_daily(
        &self,
        scope: &AuditReadScope,
        query: &AuditDailyQuery,
    ) -> Result<AuditDailyPage, StoreError> {
        if !scope.permits(&query.partition) {
            return Err(StoreError::AuditAccessDenied);
        }
        query.validate()?;
        let mut result = self
            .db
            .query(include_str!("../queries/audit/daily.surql"))
            .bind(("partition", query.partition.storage_key()))
            .bind(("from", query.from))
            .bind(("until", query.until))
            .bind(("cursor_day", query.cursor.as_ref().map(|c| c.day)))
            .bind((
                "cursor_class",
                query.cursor.as_ref().map(|c| scalar(&c.class)),
            ))
            .bind((
                "cursor_outcome",
                query.cursor.as_ref().map(|c| scalar(&c.outcome)),
            ))
            .bind(("limit", i64::from(query.limit) + 1))
            .await?
            .check()?;
        #[derive(SurrealValue)]
        struct Count {
            day: chrono::DateTime<Utc>,
            class: Value,
            outcome: Value,
            count: u64,
        }
        let mut rows: Vec<Count> = result.take(0)?;
        let more = rows.len() > usize::from(query.limit);
        rows.truncate(usize::from(query.limit));
        let counts = rows
            .into_iter()
            .map(|row| {
                Ok(AuditDailyCount {
                    partition: query.partition.clone(),
                    day: row.day,
                    class: codec::decode(row.class)?,
                    outcome: codec::decode(row.outcome)?,
                    count: row.count,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        let next = more
            .then(|| {
                counts.last().map(|row| AuditDailyCursor {
                    partition: query.partition.clone(),
                    day: row.day,
                    class: row.class,
                    outcome: row.outcome,
                })
            })
            .flatten();
        Ok(AuditDailyPage { counts, next })
    }
}

mod blocks;
pub use blocks::{AuditCommittedRecords, AuditExportRange, AuditSealLease};
mod subscriptions;
pub use subscriptions::AuditLiveChange;

#[cfg(test)]
mod write_tests {
    use super::*;

    #[test]
    fn append_record_keeps_native_identity_dates_and_frozen_draft() {
        let draft = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Server {
                server: "knowledge".parse().unwrap(),
            },
            AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            },
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )
        .build()
        .unwrap();
        let value = encode(&AuditTargetRegistry::empty(), draft.clone())
            .unwrap()
            .into_value();
        let Value::Object(write) = &value else {
            panic!("append envelope");
        };
        let Value::Object(record) = write.get("record").unwrap() else {
            panic!("append record");
        };
        assert_eq!(
            record.get("id"),
            Some(&record_id(draft.partition(), draft.id()).into_value())
        );
        assert_eq!(
            record.get("record_id"),
            Some(&SurrealUuid::from(draft.id().as_uuid()).into_value())
        );
        assert_eq!(
            record.get("occurred_at"),
            Some(&draft.occurred_at().into_value())
        );
        assert_eq!(record.get("class"), Some(&scalar(&draft.detail().class())));
        assert_eq!(record.get("outcome"), Some(&scalar(&draft.outcome())));
        assert_eq!(
            record.get("draft"),
            Some(&Document::encode(draft.clone()).into_value())
        );
        assert_eq!(record.get("actor_key"), Some(&Value::None));
        assert_eq!(record.get("profile_lookup"), Some(&Value::None));
        assert_eq!(write.get("window"), Some(&Value::None));
        assert_eq!(write.get("window_start"), Some(&Value::None));
        let decoded = AuditAppendWrite::from_value(value.clone()).unwrap();
        assert_eq!(
            decoded
                .record
                .draft
                .checked(&AuditTargetRegistry::empty())
                .unwrap(),
            draft
        );
        let mut invalid = record.clone();
        invalid.insert("occurred_at", "2026-10-05T00:00:00Z".into_value());
        assert!(AuditAppendRecord::from_value(invalid.clone().into_value()).is_err());
        invalid.remove("id");
        assert!(AuditAppendRecord::from_value(invalid.into_value()).is_err());
    }
}
