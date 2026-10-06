use super::*;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};

// Response pages always emit the cursor field, including JSON null. Require its
// presence without changing the nullable schema supplied by its actual type.
pub(super) fn require_cursor_presence(schema: &mut Schema) {
    let required = schema
        .as_object_mut()
        .expect("page schema is an object")
        .entry("required")
        .or_insert_with(|| serde_json::json!([]));
    let required = required
        .as_array_mut()
        .expect("page required fields are an array");
    if !required.iter().any(|field| field == "next_cursor") {
        required.push(serde_json::json!("next_cursor"));
    }
}

fn required_nullable_cursor(generator: &mut SchemaGenerator) -> Schema {
    <Option<String>>::json_schema(generator)
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = require_cursor_presence)]
pub struct OwnedPage<T> {
    pub items: Vec<T>,
    pub limit: usize,
    #[schemars(required, schema_with = "required_nullable_cursor")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = require_cursor_presence)]
pub struct ReleasePage {
    pub items: Vec<DatasetRelease>,
    pub limit: usize,
    #[schemars(required, schema_with = "required_nullable_cursor")]
    pub next_cursor: Option<String>,
}
