//! Explicit contextual decoding; external JSON rejects duplicate fields before Value.
use crate::*;
use serde::{
    Deserialize,
    de::{DeserializeSeed, Error},
};
use serde_json::Value;
use std::marker::PhantomData;
use veoveo_types::UniqueJsonValue;

pub struct AuditDecoder<'a> {
    registry: &'a AuditTargetRegistry,
}
impl AuditTargetRegistry {
    pub fn decoder(&self) -> AuditDecoder<'_> {
        AuditDecoder { registry: self }
    }
}
impl<'a> AuditDecoder<'a> {
    pub fn from_value<T: AuditDecode>(&self, value: Value) -> Result<T, AuditDecodeError> {
        T::decode(self.registry, value)
    }
    pub fn from_str<T: AuditDecode>(&self, input: &str) -> Result<T, AuditDecodeError> {
        let mut decoder = serde_json::Deserializer::from_str(input);
        let value = UniqueJsonValue::deserialize(&mut decoder)?;
        decoder.end()?;
        T::decode(self.registry, value.0)
    }
    pub fn seed<T: AuditDecode>(&self) -> AuditDecodeSeed<'a, T> {
        AuditDecodeSeed {
            registry: self.registry,
            marker: PhantomData,
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum AuditDecodeError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Target(#[from] AuditTargetError),
    #[error(transparent)]
    Validation(#[from] AuditValidationError),
}
pub struct AuditDecodeSeed<'a, T> {
    registry: &'a AuditTargetRegistry,
    marker: PhantomData<T>,
}
impl<'de, T: AuditDecode> DeserializeSeed<'de> for AuditDecodeSeed<'_, T> {
    type Value = T;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        T::decode(self.registry, UniqueJsonValue::deserialize(d)?.0).map_err(D::Error::custom)
    }
}
mod sealed {
    pub trait Sealed {}
}
/// Implemented only by the admitted Audit models.
pub trait AuditDecode: sealed::Sealed + Sized {
    fn decode(registry: &AuditTargetRegistry, value: Value) -> Result<Self, AuditDecodeError>;
}
impl sealed::Sealed for AuditTarget {}
impl AuditDecode for AuditTarget {
    fn decode(registry: &AuditTargetRegistry, value: Value) -> Result<Self, AuditDecodeError> {
        Ok(registry.admit(value)?)
    }
}

use chrono::{DateTime, Utc};
use veoveo_types::PrincipalId;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditDraftWireInput {
    id: AuditRecordId,
    schema: AuditSchema,
    partition: AuditPartition,
    actor: Option<AuditActor>,
    authority: AuditAuthority,
    target: Value,
    detail: AuditDetail,
    outcome: AuditOutcome,
    reason: AuditReason,
    request: AuditRequest,
    occurred_at: DateTime<Utc>,
    latency_ms: Option<u64>,
}
impl sealed::Sealed for AuditDraft {}
impl AuditDecode for AuditDraft {
    fn decode(registry: &AuditTargetRegistry, value: Value) -> Result<Self, AuditDecodeError> {
        let wire: AuditDraftWireInput = serde_json::from_value(value)?;
        let admitted = crate::model::AuditDraftWire {
            id: wire.id,
            schema: wire.schema,
            partition: wire.partition,
            actor: wire.actor,
            authority: wire.authority,
            target: registry.admit(wire.target)?,
            detail: wire.detail,
            outcome: wire.outcome,
            reason: wire.reason,
            request: wire.request,
            occurred_at: wire.occurred_at,
            latency_ms: wire.latency_ms,
        };
        Ok(admitted.try_into()?)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditRecordInput {
    draft: Value,
    recorded_at: DateTime<Utc>,
}
impl sealed::Sealed for AuditRecord {}
impl AuditDecode for AuditRecord {
    fn decode(registry: &AuditTargetRegistry, value: Value) -> Result<Self, AuditDecodeError> {
        let wire: AuditRecordInput = serde_json::from_value(value)?;
        let admitted = Self {
            draft: AuditDraft::decode(registry, wire.draft)?,
            recorded_at: wire.recorded_at,
        };
        Ok(admitted)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditQueryInput {
    order: AuditOrder,
    partition: AuditPartition,
    cursor: Option<AuditCursor>,
    class: Option<AuditClass>,
    actor: Option<PrincipalId>,
    target: Option<Value>,
    outcome: Option<AuditOutcome>,
    trace: Option<AuditTraceId>,
    from: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    limit: u16,
}
impl sealed::Sealed for AuditQuery {}
impl AuditDecode for AuditQuery {
    fn decode(registry: &AuditTargetRegistry, value: Value) -> Result<Self, AuditDecodeError> {
        let wire: AuditQueryInput = serde_json::from_value(value)?;
        let admitted = Self {
            order: wire.order,
            partition: wire.partition,
            cursor: wire.cursor,
            class: wire.class,
            actor: wire.actor,
            target: wire.target.map(|v| registry.admit(v)).transpose()?,
            outcome: wire.outcome,
            trace: wire.trace,
            from: wire.from,
            until: wire.until,
            limit: wire.limit,
        };
        admitted.validate()?;
        Ok(admitted)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditPageInput {
    records: Vec<Value>,
    next: Option<AuditCursor>,
}
impl sealed::Sealed for AuditPage {}
impl AuditDecode for AuditPage {
    fn decode(registry: &AuditTargetRegistry, value: Value) -> Result<Self, AuditDecodeError> {
        let wire: AuditPageInput = serde_json::from_value(value)?;
        let admitted = Self {
            records: wire
                .records
                .into_iter()
                .map(|v| AuditRecord::decode(registry, v))
                .collect::<Result<_, _>>()?,
            next: wire.next,
        };
        Ok(admitted)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuditRecordSummaryInput {
    id: AuditRecordId,
    occurred_at: DateTime<Utc>,
    actor: Option<veoveo_types::PrincipalId>,
    class: AuditClass,
    detail: AuditDetail,
    target: Value,
    outcome: AuditOutcome,
    reason: AuditReason,
    source_ip: Option<std::net::IpAddr>,
    trace_id: AuditTraceId,
    request_id: AuditRequestId,
}
impl sealed::Sealed for AuditRecordSummary {}
impl AuditDecode for AuditRecordSummary {
    fn decode(registry: &AuditTargetRegistry, value: Value) -> Result<Self, AuditDecodeError> {
        let wire: AuditRecordSummaryInput = serde_json::from_value(value)?;
        let admitted = Self {
            id: wire.id,
            occurred_at: wire.occurred_at,
            actor: wire.actor,
            class: wire.class,
            detail: wire.detail,
            target: registry.admit(wire.target)?,
            outcome: wire.outcome,
            reason: wire.reason,
            source_ip: wire.source_ip,
            trace_id: wire.trace_id,
            request_id: wire.request_id,
        };
        Ok(admitted)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditSummaryPageInput {
    records: Vec<Value>,
    next: Option<AuditCursor>,
}
impl sealed::Sealed for AuditSummaryPage {}
impl AuditDecode for AuditSummaryPage {
    fn decode(registry: &AuditTargetRegistry, value: Value) -> Result<Self, AuditDecodeError> {
        let wire: AuditSummaryPageInput = serde_json::from_value(value)?;
        let admitted = Self {
            records: wire
                .records
                .into_iter()
                .map(|v| AuditRecordSummary::decode(registry, v))
                .collect::<Result<_, _>>()?,
            next: wire.next,
        };
        Ok(admitted)
    }
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ExportInput {
    Header {
        query: Value,
        first: Option<AuditBlockSequence>,
        checkpoint: Option<AuditCheckpoint>,
    },
    Record {
        record: Value,
    },
    Complete {
        records: u64,
        checkpoint: Option<AuditCheckpoint>,
    },
}
impl sealed::Sealed for AuditExportLine {}
impl AuditDecode for AuditExportLine {
    fn decode(registry: &AuditTargetRegistry, value: Value) -> Result<Self, AuditDecodeError> {
        Ok(match serde_json::from_value::<ExportInput>(value)? {
            ExportInput::Header {
                query,
                first,
                checkpoint,
            } => Self::Header {
                query: Box::new(AuditQuery::decode(registry, query)?),
                first,
                checkpoint,
            },
            ExportInput::Record { record } => Self::Record {
                record: Box::new(AuditRecord::decode(registry, record)?),
            },
            ExportInput::Complete {
                records,
                checkpoint,
            } => Self::Complete {
                records,
                checkpoint,
            },
        })
    }
}
