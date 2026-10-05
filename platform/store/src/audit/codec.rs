//! Typed audit contracts cross the driver boundary through one checked JSON adapter.
use crate::{RecordId, StoreError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use veoveo_audit_contract::{AuditDecodeError, AuditDraft, AuditRecord, AuditTargetRegistry};

#[derive(Debug, Clone)]
pub(super) struct Document(pub serde_json::Value);
impl Document {
    pub fn encode(draft: AuditDraft) -> Self {
        Self(serde_json::to_value(draft).expect("typed audit draft"))
    }
    pub fn checked(self, registry: &AuditTargetRegistry) -> Result<AuditDraft, StoreError> {
        registry
            .decoder()
            .from_value(self.0)
            .map_err(|error| match error {
                AuditDecodeError::Target(error) => StoreError::AuditTarget(error),
                _ => StoreError::AuditIntegrity,
            })
    }
}
impl SurrealValue for Document {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        crate::json_value::into_surreal(
            serde_json::to_value(self.0).expect("typed audit serialization"),
        )
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        let json = crate::json_value::from_surreal(value)?;
        Ok(Self(json))
    }
}
#[derive(Debug, Clone, SurrealValue)]
pub(super) struct Row {
    pub id: RecordId,
    pub partition: String,
    pub draft: Document,
    pub target_ref: Option<RecordId>,
    pub recorded_at: DateTime<Utc>,
}
impl Row {
    pub fn checked(self, registry: &AuditTargetRegistry) -> Result<AuditRecord, StoreError> {
        let draft = self.draft.checked(registry)?;
        if self.id != super::record_id(draft.partition(), draft.id())
            || self.partition != draft.partition().storage_key()
            || self.target_ref != super::target_reference(draft.target())?
        {
            return Err(StoreError::AuditIntegrity);
        }
        Ok(AuditRecord {
            draft,
            recorded_at: self.recorded_at,
        })
    }
}
pub(super) fn scalar<T: Serialize>(value: &T) -> Value {
    crate::json_value::into_surreal(
        serde_json::to_value(value).expect("typed audit field serialization"),
    )
}
pub(super) fn decode<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, StoreError> {
    let json = crate::json_value::from_surreal(value).map_err(|_| StoreError::AuditIntegrity)?;
    serde_json::from_value(json).map_err(|_| StoreError::AuditIntegrity)
}

/// Checked domain attribution encoded as one driver-owned object. No MCP dependency.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct AuditContextRecord(pub veoveo_audit_contract::AuditContext);
impl SurrealValue for AuditContextRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        scalar(&self.0)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        let json = crate::json_value::from_surreal(value)?;
        serde_json::from_value(json)
            .map(Self)
            .map_err(|_| Error::internal("invalid audit attribution".into()))
    }
}
