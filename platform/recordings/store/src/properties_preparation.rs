//! Native preparation stores the checked RRD properties preimage before effects.
use crate::{RecordingDatasetId, RecordingId, RecordingLayerKind, RecordingLayerRecord};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use veoveo_recording_contract::RecordingProperties;
use veoveo_types::Sha256Digest;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "PreparationWire", into = "PreparationWire")]
pub struct RecordingPropertiesPreparation {
    version: u8,
    pub body: RecordingProperties,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparationWire {
    version: u8,
    body: RecordingProperties,
}
impl TryFrom<PreparationWire> for RecordingPropertiesPreparation {
    type Error = crate::RecordingStoreError;
    fn try_from(value: PreparationWire) -> Result<Self, Self::Error> {
        if value.version != 1 {
            return Err(crate::RecordingStoreError::InvalidRecordingField {
                field: "properties_preparation",
                reason: "unsupported preparation version",
            });
        }
        Ok(Self {
            version: value.version,
            body: value.body,
        })
    }
}
impl From<RecordingPropertiesPreparation> for PreparationWire {
    fn from(value: RecordingPropertiesPreparation) -> Self {
        Self {
            version: value.version,
            body: value.body,
        }
    }
}

impl RecordingPropertiesPreparation {
    pub fn new(body: RecordingProperties) -> Self {
        Self { version: 1, body }
    }
}
impl SurrealValue for RecordingPropertiesPreparation {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        veoveo_platform_store::native_json_into_value(
            serde_json::to_value(self).expect("checked properties preparation"),
        )
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        // The two genuinely optional revision dictionaries omit native NONE;
        // required body members still undergo canonical owner admission.
        let value: Self =
            serde_json::from_value(veoveo_platform_store::native_json_from_value(value)?)
                .map_err(|_| Error::internal("invalid Recording properties preparation".into()))?;
        Ok(value)
    }
}

pub fn source_layer_manifest_digest(
    dataset_id: RecordingDatasetId,
    recording_id: RecordingId,
    layers: &[RecordingLayerRecord],
) -> Sha256Digest {
    let mut digest = Sha256::new();
    digest.update(dataset_id.to_string());
    digest.update([0]);
    digest.update(recording_id.to_string());
    for layer in layers
        .iter()
        .filter(|layer| layer.kind != RecordingLayerKind::Properties)
    {
        digest.update([0]);
        digest.update(layer.layer_name.as_bytes());
        digest.update(layer.byte_len.to_be_bytes());
        digest.update(layer.message_count.to_be_bytes());
        if let Some(sha256) = &layer.sha256 {
            digest.update(sha256.as_bytes());
        }
    }
    Sha256Digest::from_bytes(digest.finalize().into())
}
