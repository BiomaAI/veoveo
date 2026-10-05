//! Retained owner envelopes preserve admitted JSON instead of projecting defaults.
use super::{DurableTravelModelRequest, Input};
use crate::contract::{MapTaskProduct, TravelModelRecord};
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use veoveo_platform_store::{native_json_from_value_strict, native_json_into_value};
use veoveo_task_runtime::TaskError;

#[derive(Clone)]
pub(crate) struct TravelModelInputRecord {
    original: serde_json::Value,
    request: DurableTravelModelRequest,
}
impl TravelModelInputRecord {
    pub(crate) fn new(original: serde_json::Value) -> Result<Self, TaskError> {
        let Input::BuildTravelModel(request) = serde_json::from_value(original.clone())?;
        request
            .input
            .validate()
            .map_err(|e| TaskError::InvalidRecord(e.to_string()))?;
        Ok(Self { original, request })
    }
    pub(crate) fn request(&self) -> &DurableTravelModelRequest {
        &self.request
    }
}
impl SurrealValue for TravelModelInputRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native_json_into_value(self.original)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        Self::new(native_json_from_value_strict(value)?)
            .map_err(|_| Error::internal("invalid Map retained input".into()))
    }
}

pub(crate) struct TravelModelResultRecord {
    original: serde_json::Value,
    output: TravelModelRecord,
}
impl TravelModelResultRecord {
    pub(crate) fn new(original: serde_json::Value) -> Result<Self, TaskError> {
        let envelope: rmcp::model::CallToolResult = serde_json::from_value(original.clone())?;
        if envelope.is_error == Some(true) {
            return Err(TaskError::InvalidRecord(
                "Map product is a tool error".into(),
            ));
        }
        let product: MapTaskProduct<TravelModelRecord> =
            serde_json::from_value(envelope.structured_content.ok_or_else(|| {
                TaskError::InvalidRecord("Map travel result has no structured content".into())
            })?)?;
        let output = product.into_output();
        output
            .validate_identity()
            .map_err(|e| TaskError::InvalidRecord(e.to_string()))?;
        Ok(Self { original, output })
    }
    pub(crate) fn output(&self) -> &TravelModelRecord {
        &self.output
    }
    pub(crate) fn into_output(self) -> TravelModelRecord {
        self.output
    }
}
impl SurrealValue for TravelModelResultRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native_json_into_value(self.original)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        Self::new(native_json_from_value_strict(value)?)
            .map_err(|_| Error::internal("invalid Map retained result".into()))
    }
}
