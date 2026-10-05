//! Media owns the known prediction envelope inside the kernel's opaque provider journal.
use crate::{
    contract::{MediaModelId, MediaPredictionId},
    provider::{Prediction, PredictionUrls},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use surrealdb::types::{Error, Kind, SurrealValue};
use veoveo_platform_store::{OpenObject, native_json_from_value, native_json_into_value};

#[derive(Serialize, Deserialize)]
#[serde(remote = "Prediction", deny_unknown_fields)]
struct PredictionWire {
    id: MediaPredictionId,
    model: MediaModelId,
    #[serde(default)]
    outputs: Vec<String>,
    #[serde(default)]
    #[serde(deserialize_with = "stored_urls")]
    urls: Option<PredictionUrls>,
    status: String,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    error: Option<String>,
    #[serde(rename = "executionTime", default)]
    execution_time: Option<f64>,
    #[serde(default)]
    timings: Option<Value>,
    /// Original request input; present in webhook payloads.
    #[serde(default)]
    input: Option<Value>,
}

fn stored_urls<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<PredictionUrls>, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Urls {
        get: String,
    }
    Option::<Urls>::deserialize(deserializer)
        .map(|urls| urls.map(|urls| PredictionUrls { get: urls.get }))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct PredictionRecord(#[serde(with = "PredictionWire")] pub(crate) Prediction);

impl PredictionRecord {
    /// The kernel deliberately leaves each owner's extension payload open.
    pub(crate) fn from_payload(payload: OpenObject) -> Result<Self, serde_json::Error> {
        serde_json::from_value(Value::Object(payload.into_map().into_iter().collect()))
    }
}

impl SurrealValue for PredictionRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &surrealdb::types::Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> surrealdb::types::Value {
        native_json_into_value(serde_json::to_value(self).expect("Media prediction serializes"))
    }
    fn from_value(value: surrealdb::types::Value) -> Result<Self, Error> {
        serde_json::from_value(native_json_from_value(value)?)
            .map_err(|e| Error::internal(format!("invalid stored Media prediction: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prediction_driver_preserves_open_input_and_unknown_nonterminal_status() {
        let wire = serde_json::json!({"id":"prediction-1", "model":"owner/model", "outputs":[], "urls":{"get":"https://provider.example/result"}, "status":"future-status", "created_at":null, "error":null, "executionTime":3.5, "timings":{"future":{"metric":4}}, "input":{"provider_extension":[true, null, {"x":1}]}});
        let value = native_json_into_value(wire);
        let prediction = PredictionRecord::from_value(value.clone()).unwrap();
        assert!(prediction.0.terminal_outcome().is_none());
        assert_eq!(prediction.into_value(), value);
    }
    #[test]
    fn prediction_driver_rejects_unknown_envelope_fields_and_native_values() {
        let baseline =
            serde_json::json!({"id":"prediction-1", "model":"owner/model", "status":"completed"});
        for (path, value) in [
            ("extra", serde_json::json!(true)),
            ("urls", serde_json::json!({"get":"x", "extra":true})),
            ("outputs", serde_json::json!([42])),
        ] {
            let mut wire = baseline.clone();
            wire[path] = value;
            assert!(PredictionRecord::from_value(native_json_into_value(wire)).is_err());
        }
        assert!(
            PredictionRecord::from_value(surrealdb::types::Value::RecordId(
                surrealdb::types::RecordId::new("prediction", "fixture")
            ))
            .is_err()
        );
    }
}
