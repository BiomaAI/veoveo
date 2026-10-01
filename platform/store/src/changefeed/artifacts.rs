//! Artifact invalidations share one record-link decoder.
use crate::{ChangefeedEntry, StoreError};
use surrealdb::types::{RecordIdKey, Value};
use veoveo_artifact_contract::ArtifactId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactChange {
    pub artifact_id: ArtifactId,
}
impl ArtifactChange {
    pub fn decode(entry: &ChangefeedEntry) -> Result<Option<Self>, StoreError> {
        let record = match entry.table() {
            Some("artifact_occurrence") => entry.record_id(),
            Some("artifact_grant" | "share_link") => {
                let row = match entry {
                    ChangefeedEntry::Upsert(row) => Some(row),
                    ChangefeedEntry::Delete { original, .. } => original.as_ref(),
                    ChangefeedEntry::Definition => None,
                };
                match row.map(|row| row.get("artifact")) {
                    Some(Value::RecordId(record)) => Some(record),
                    _ => None,
                }
            }
            _ => return Ok(None),
        }
        .filter(|record| record.table.as_str() == "artifact_occurrence");
        let id = record.and_then(|record| match &record.key {
            RecordIdKey::Uuid(id) => ArtifactId::try_from(**id).ok(),
            _ => None,
        }).ok_or(StoreError::InvalidChangefeedEntry { reason: "Artifact change requires its typed occurrence link, including delete originals" })?;
        Ok(Some(Self { artifact_id: id }))
    }
}
