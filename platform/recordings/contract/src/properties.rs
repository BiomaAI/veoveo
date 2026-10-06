//! Sealed properties preserve producer timestamp text and RRD JSON hash preimages.
use crate::{RecordingContractError, RecordingDatasetId, RecordingId, RecordingState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use veoveo_types::Sha256Digest;
const MAX_METADATA_REVISIONS: usize = 64;
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingPropertiesBuilder {
    #[serde(with = "property_dataset_id")]
    #[schemars(with = "RecordingDatasetId")]
    pub dataset_id: RecordingDatasetId,
    #[serde(with = "property_recording_id")]
    #[schemars(with = "RecordingId")]
    pub recording_id: RecordingId,
    pub dataset_key: String,
    pub producer_recording_key: String,
    #[serde(with = "property_state")]
    #[schemars(schema_with = "property_state::schema")]
    pub lifecycle_state: RecordingState,
    pub started_at: String,
    pub ended_at: String,
    pub sealed_at: String,
    #[schemars(range(min = 0))]
    pub source_revision: i64,
    #[serde(with = "veoveo_types::sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub immutable_manifest_digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_revisions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment_revisions: BTreeMap<String, String>,
}

impl RecordingPropertiesBuilder {
    pub fn build(self) -> Result<RecordingProperties, RecordingContractError> {
        veoveo_types::Checked::new(self).map(RecordingProperties)
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct RecordingProperties(veoveo_types::Checked<RecordingPropertiesBuilder>);
impl std::ops::Deref for RecordingProperties {
    type Target = RecordingPropertiesBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl veoveo_types::Check for RecordingPropertiesBuilder {
    type Error = RecordingContractError;
    fn check(&self) -> Result<(), Self::Error> {
        validate(self).map_err(|_| RecordingContractError::Seal)
    }
}

fn validate(properties: &RecordingPropertiesBuilder) -> Result<(), ()> {
    for value in [
        properties.dataset_key.as_str(),
        properties.producer_recording_key.as_str(),
        properties.started_at.as_str(),
        properties.ended_at.as_str(),
        properties.sealed_at.as_str(),
    ] {
        if value.trim().is_empty() || value.len() > 512 {
            return Err(());
        }
    }
    let started = chrono::DateTime::parse_from_rfc3339(&properties.started_at).map_err(|_| ())?;
    let ended = chrono::DateTime::parse_from_rfc3339(&properties.ended_at).map_err(|_| ())?;
    chrono::DateTime::parse_from_rfc3339(&properties.sealed_at).map_err(|_| ())?;
    if ended < started
        || properties.lifecycle_state != RecordingState::Sealed
        || properties.source_revision < 0
    {
        return Err(());
    }
    validate_revisions(&properties.model_revisions)?;
    validate_revisions(&properties.environment_revisions)?;
    if serde_json::to_vec(properties).map_err(|_| ())?.len() > 64 * 1024 {
        return Err(());
    }
    Ok(())
}

fn validate_revisions(values: &BTreeMap<String, String>) -> Result<(), ()> {
    if values.len() > MAX_METADATA_REVISIONS {
        return Err(());
    }
    for (key, value) in values {
        if key.trim().is_empty() || key.len() > 128 || value.trim().is_empty() || value.len() > 256
        {
            return Err(());
        }
    }
    Ok(())
}

// Properties retain their original UUID binary representation; public Recording
// IDs elsewhere retain the owner's string-only binary profile.
mod property_dataset_id {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        value: &RecordingDatasetId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_uuid().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<RecordingDatasetId, D::Error> {
        if deserializer.is_human_readable() {
            RecordingDatasetId::deserialize(deserializer)
        } else {
            RecordingDatasetId::try_from(uuid::Uuid::deserialize(deserializer)?)
                .map_err(serde::de::Error::custom)
        }
    }
}
mod property_recording_id {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        value: &RecordingId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_uuid().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<RecordingId, D::Error> {
        if deserializer.is_human_readable() {
            RecordingId::deserialize(deserializer)
        } else {
            RecordingId::try_from(uuid::Uuid::deserialize(deserializer)?)
                .map_err(serde::de::Error::custom)
        }
    }
}
mod property_state {
    use super::*;
    pub fn schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = <String as schemars::JsonSchema>::json_schema(generator);
        schema.insert(
            "const".to_owned(),
            serde_json::json!(RecordingState::Sealed.to_string()),
        );
        schema
    }
    pub fn serialize<S: serde::Serializer>(
        value: &RecordingState,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.to_string().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<RecordingState, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value == RecordingState::Sealed.to_string() {
            Ok(RecordingState::Sealed)
        } else {
            Err(serde::de::Error::custom(RecordingContractError::Seal))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn producer() -> RecordingPropertiesBuilder {
        RecordingPropertiesBuilder {
            dataset_id: RecordingDatasetId::new(),
            recording_id: RecordingId::new(),
            dataset_key: "open upstream application/name".into(),
            producer_recording_key: "open upstream recording/name".into(),
            lifecycle_state: RecordingState::Sealed,
            started_at: "2026-10-06T01:00:00+00:00".into(),
            ended_at: "2026-10-06T01:05:00+00:00".into(),
            // Producer wall clocks do not establish a seal-after-end invariant.
            sealed_at: "2026-10-06T01:04:00Z".into(),
            source_revision: 0,
            immutable_manifest_digest: Sha256Digest::from_bytes([0xab; 32]),
            model_revisions: BTreeMap::from([("external/model".into(), "opaque revision".into())]),
            environment_revisions: BTreeMap::new(),
        }
    }

    #[test]
    fn producer_profile_preserves_timestamp_and_bare_digest_bytes() {
        let builder = producer();
        let original = serde_json::to_vec(&builder).unwrap();
        let properties = builder.build().unwrap();
        assert_eq!(serde_json::to_vec(&properties).unwrap(), original);
        let decoded: RecordingProperties = serde_json::from_slice(&original).unwrap();
        assert_eq!(properties, decoded);
        let value = serde_json::to_value(&decoded).unwrap();
        assert_eq!(value["started_at"], "2026-10-06T01:00:00+00:00");
        assert_eq!(value["sealed_at"], "2026-10-06T01:04:00Z");
        assert_eq!(value["immutable_manifest_digest"], "ab".repeat(32));
        assert_eq!(value["source_revision"], 0);
        let schema = serde_json::to_value(schemars::schema_for!(RecordingProperties)).unwrap();
        assert_eq!(schema["properties"]["lifecycle_state"]["const"], "sealed");
        assert_eq!(schema["properties"]["source_revision"]["minimum"], 0);
        assert!(value.get("environment_revisions").is_none());
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(RecordingProperties)).unwrap()["type"],
            "object"
        );
    }

    #[test]
    fn constructors_and_decoders_reject_corrupt_sealed_properties() {
        let original = serde_json::to_value(producer()).unwrap();
        for (field, corrupt) in [
            ("started_at", serde_json::json!("not a timestamp")),
            ("ended_at", serde_json::json!("2026-10-06T00:59:59Z")),
            ("sealed_at", serde_json::json!("2026-99-99T00:00:00Z")),
            ("lifecycle_state", serde_json::json!("live")),
            ("source_revision", serde_json::json!(-1)),
            (
                "dataset_id",
                serde_json::json!("67e55044-10b1-426f-9247-bb680e5fe0c8"),
            ),
            ("recording_id", serde_json::json!("not a recording")),
            (
                "immutable_manifest_digest",
                serde_json::json!("AB".repeat(32)),
            ),
            ("dataset_key", serde_json::json!(" ")),
            ("model_revisions", serde_json::json!({"": "revision"})),
            ("environment_revisions", serde_json::json!({"model": " "})),
        ] {
            let mut value = original.clone();
            value[field] = corrupt;
            assert!(
                serde_json::from_value::<RecordingProperties>(value.clone()).is_err(),
                "{field}"
            );
            if let Ok(builder) = serde_json::from_value::<RecordingPropertiesBuilder>(value) {
                assert!(builder.build().is_err(), "{field}");
            }
        }
        let mut value = original;
        value["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RecordingProperties>(value).is_err());
    }
    struct BinaryUuid<'a>(&'a [u8]);
    impl<'de> serde::Deserializer<'de> for BinaryUuid<'de> {
        type Error = serde::de::value::Error;
        fn deserialize_any<V: serde::de::Visitor<'de>>(
            self,
            visitor: V,
        ) -> Result<V::Value, Self::Error> {
            visitor.visit_borrowed_bytes(self.0)
        }
        fn is_human_readable(&self) -> bool {
            false
        }
        serde::forward_to_deserialize_any! { bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any }
    }
    #[test]
    fn property_binary_uuid_adapters_preserve_bytes_and_apply_owner_admission() {
        let dataset = RecordingDatasetId::new();
        let recording = RecordingId::new();
        assert_eq!(
            property_dataset_id::deserialize(BinaryUuid(dataset.as_uuid().as_bytes())).unwrap(),
            dataset
        );
        assert_eq!(
            property_recording_id::deserialize(BinaryUuid(recording.as_uuid().as_bytes())).unwrap(),
            recording
        );
        let wrong = uuid::Uuid::from_u128(0x67e5504410b1426f9247bb680e5fe0c8);
        assert!(property_dataset_id::deserialize(BinaryUuid(wrong.as_bytes())).is_err());
        assert!(property_recording_id::deserialize(BinaryUuid(wrong.as_bytes())).is_err());
        // Binary state remains a string, never a new enum ordinal.
        let state = serde::de::value::StrDeserializer::<serde::de::value::Error>::new("sealed");
        assert_eq!(
            property_state::deserialize(state).unwrap(),
            RecordingState::Sealed
        );
        let state = serde::de::value::U32Deserializer::<serde::de::value::Error>::new(3);
        assert!(property_state::deserialize(state).is_err());
    }
}
