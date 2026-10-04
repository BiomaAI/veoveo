//! Recording producer transport identities and their existing wire admission profiles.
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use veoveo_types::{
    IdentifierError,
    identifier_syntax::{validate_claim_text, validate_path_id},
};

fn validate_dataset_name(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}
fn validate_stream_id(value: &str) -> Result<(), IdentifierError> {
    let uuid = uuid::Uuid::parse_str(value)
        .map_err(|_| IdentifierError::new(value, "must be a UUIDv7"))?;
    if uuid.get_version_num() != 7 {
        return Err(IdentifierError::new(value, "must be a UUIDv7"));
    }
    Ok(())
}

/// Configured identity of one governed recording producer.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_path_id, error = IdentifierError)]
pub struct RecordingProducerId(String);
impl JsonSchema for RecordingProducerId {
    fn schema_name() -> Cow<'static, str> {
        "RecordingProducerId".into()
    }
    fn schema_id() -> Cow<'static, str> {
        "veoveo_mcp_contract::gateway::ids::RecordingProducerId".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = String::json_schema(generator);
        schema.insert(
            "description".into(),
            serde_json::Value::String(
                "Configured identity of one governed recording producer.".into(),
            ),
        );
        schema
    }
}

/// Installation-owned dataset name assigned to a recording producer.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_dataset_name, error = IdentifierError)]
pub struct RecordingDatasetName(String);
impl JsonSchema for RecordingDatasetName {
    fn schema_name() -> Cow<'static, str> {
        "RecordingDatasetName".into()
    }
    fn schema_id() -> Cow<'static, str> {
        "veoveo_mcp_contract::gateway::ids::RecordingDatasetName".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = String::json_schema(generator);
        schema.insert(
            "description".into(),
            serde_json::Value::String(
                "Installation-owned dataset name assigned to a recording producer.".into(),
            ),
        );
        schema
    }
}

/// Rerun application id admitted for a recording producer.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_claim_text, error = IdentifierError)]
pub struct RecordingApplicationId(String);
impl JsonSchema for RecordingApplicationId {
    fn schema_name() -> Cow<'static, str> {
        "RecordingApplicationId".into()
    }
    fn schema_id() -> Cow<'static, str> {
        "veoveo_mcp_contract::gateway::ids::RecordingApplicationId".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = String::json_schema(generator);
        schema.insert(
            "description".into(),
            serde_json::Value::String(
                "Rerun application id admitted for a recording producer.".into(),
            ),
        );
        schema
    }
}

/// Canonical UUIDv7 identity of one authenticated recording ingest stream.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_stream_id, error = IdentifierError)]
pub struct RecordingIngestStreamId(String);
impl JsonSchema for RecordingIngestStreamId {
    fn schema_name() -> Cow<'static, str> {
        "RecordingIngestStreamId".into()
    }
    fn schema_id() -> Cow<'static, str> {
        "veoveo_mcp_contract::gateway::ids::RecordingIngestStreamId".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = String::json_schema(generator);
        schema.insert(
            "description".into(),
            serde_json::Value::String(
                "Canonical UUIDv7 identity of one authenticated recording ingest stream.".into(),
            ),
        );
        schema
    }
}
