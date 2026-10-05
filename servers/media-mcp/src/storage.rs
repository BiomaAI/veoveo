//! Media-owned database records and persistence identities.
mod prediction;
use chrono::{DateTime, Utc};
pub(crate) use prediction::PredictionRecord;
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::PersistenceIds;
use veoveo_platform_store::{OpenObject, RedactedSecret};
use veoveo_types::id;

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "media_task_context"
)]
pub struct MediaTaskContextId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "media_usage")]
pub struct MediaUsageId(Uuid);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum MediaUsageKind {
    #[vocabulary(rename = "estimate")]
    Estimate,
    #[vocabulary(rename = "actual")]
    Actual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MediaTaskContextRecord {
    pub id: RecordId,
    pub task: RecordId,
    pub tenant: RecordId,
    pub capability: RecordId,
    pub capability_secret: RedactedSecret,
    pub capability_expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MediaUsageRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub task: RecordId,
    pub provider_job: Option<RecordId>,
    pub source_id: Option<String>,
    pub model_id: String,
    pub kind: MediaUsageKind,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub metadata: OpenObject,
    pub recorded_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_persistence_ids_preserve_native_table_and_wire_profiles() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let context = MediaTaskContextId::from_uuid(uuid);
        let usage = MediaUsageId::from_uuid(uuid);
        assert_eq!(context.record_id().table.as_str(), "media_task_context");
        assert_eq!(usage.record_id().table.as_str(), "media_usage");
        assert_eq!(
            serde_json::to_string(&context).unwrap(),
            serde_json::to_string(&uuid).unwrap()
        );
        assert_eq!(
            MediaTaskContextId::from_value(context.into_value()).unwrap(),
            context
        );
        assert_eq!(MediaUsageId::from_value(usage.into_value()).unwrap(), usage);
        // Bare native UUIDs preserve the established ID wire profile; record values carry table identity.
        assert!(MediaUsageId::from_value(context.record_id().into_value()).is_err());
    }
    #[test]
    fn usage_vocabulary_preserves_wire_and_native_spelling() {
        for (kind, text) in [
            (MediaUsageKind::Estimate, "estimate"),
            (MediaUsageKind::Actual, "actual"),
        ] {
            assert_eq!(kind.into_value(), text.to_owned().into_value());
            assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{text}\""));
            assert_eq!(MediaUsageKind::from_value(kind.into_value()).unwrap(), kind);
        }
    }
}
