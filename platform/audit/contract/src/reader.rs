//! Reader projections preserve domain identities and the closed activity vocabulary.
use crate::*;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditRecordSummary {
    pub id: AuditRecordId,
    pub occurred_at: DateTime<Utc>,
    pub actor: Option<veoveo_types::PrincipalId>,
    pub class: AuditClass,
    pub detail: AuditDetail,
    pub target: AuditTarget,
    pub outcome: AuditOutcome,
    pub reason: AuditReason,
    pub source_ip: Option<std::net::IpAddr>,
    pub trace_id: AuditTraceId,
    pub request_id: AuditRequestId,
}
impl From<AuditRecord> for AuditRecordSummary {
    fn from(record: AuditRecord) -> Self {
        let draft = record.draft;
        Self {
            id: draft.id(),
            occurred_at: draft.occurred_at(),
            actor: draft.actor().map(|actor| actor.principal.clone()),
            class: draft.detail().class(),
            detail: draft.detail().clone(),
            target: draft.target().clone(),
            outcome: draft.outcome(),
            reason: draft.reason(),
            source_ip: draft.request().source_ip,
            trace_id: draft.request().trace_id.clone(),
            request_id: draft.request().id,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditSummaryPage {
    pub records: Vec<AuditRecordSummary>,
    pub next: Option<AuditCursor>,
}
/// Reference to the committed access record, checked against the current caller
/// and selected partition on each read. This identifier grants no authority.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditViewSession {
    pub id: AuditRecordId,
    pub partition: AuditPartition,
    pub expires_at: DateTime<Utc>,
}

/// JSON Lines readers accept an export only after its matching completion footer.
/// A record's source checkpoint is fixed before the first line is emitted.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuditExportLine {
    Header {
        query: Box<AuditQuery>,
        first: Option<AuditBlockSequence>,
        checkpoint: Option<AuditCheckpoint>,
    },
    Record {
        record: Box<AuditRecord>,
    },
    Complete {
        records: u64,
        checkpoint: Option<AuditCheckpoint>,
    },
}

/// Schema root for generated clients; this envelope is not an API response.
#[derive(JsonSchema)]
#[allow(dead_code)]
struct AuditReaderApi {
    partitions: Vec<AuditPartition>,
    query: AuditQuery,
    daily_query: AuditDailyQuery,
    page: AuditSummaryPage,
    daily_page: AuditDailyPage,
    view: AuditViewSession,
}
pub fn reader_schema() -> schemars::Schema {
    schemars::schema_for!(AuditReaderApi)
}
