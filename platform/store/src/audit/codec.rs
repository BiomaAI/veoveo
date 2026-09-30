//! Typed audit contracts cross the driver boundary through one checked JSON adapter.
use crate::{RecordId, StoreError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use veoveo_audit_contract::{AuditDraft, AuditRecord};

#[derive(Debug, Clone)]
pub(super) struct Document(pub AuditDraft);
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
        serde_json::from_value(json)
            .map(Self)
            .map_err(|_| Error::internal("invalid typed audit document".into()))
    }
}
#[derive(Debug, Clone, SurrealValue)]
pub(super) struct Row {
    pub id: RecordId,
    pub partition: String,
    pub draft: Document,
    pub recorded_at: DateTime<Utc>,
}
impl Row {
    pub fn checked(self) -> Result<AuditRecord, StoreError> {
        if self.id != super::record_id(self.draft.0.partition(), self.draft.0.id())
            || self.partition != self.draft.0.partition().storage_key()
        {
            return Err(StoreError::AuditIntegrity);
        }
        Ok(AuditRecord {
            draft: self.draft.0,
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
