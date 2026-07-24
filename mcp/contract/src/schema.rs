use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};

use rmcp::model::JsonObject;
use schemars::{JsonSchema, generate::SchemaSettings};

/// Generates the canonical JSON Schema 2020-12 contract for an MCP tool input.
///
/// Tool arguments are deliberately self-contained because clients use these
/// schemas to construct calls, not only to validate completed JSON documents.
pub fn mcp_input_schema<T>() -> Arc<JsonObject>
where
    T: JsonSchema + Any,
{
    thread_local! {
        static CACHE: RwLock<HashMap<TypeId, Arc<JsonObject>>> = RwLock::new(HashMap::new());
    }

    CACHE.with(|cache| {
        if let Some(schema) = cache
            .read()
            .expect("MCP input schema cache lock poisoned")
            .get(&TypeId::of::<T>())
        {
            return schema.clone();
        }

        let mut settings = SchemaSettings::draft2020_12();
        settings.inline_subschemas = true;
        let schema = settings.into_generator().into_root_schema_for::<T>();
        let mut schema = serde_json::to_value(schema).expect("MCP input schema must serialize");
        expose_union_types(&mut schema);
        let mut schema = schema
            .as_object()
            .cloned()
            .expect("MCP input schema root must be an object");
        schema.remove("title");
        schema.remove("description");
        validate_client_facing_input::<T>(&schema);

        let schema = Arc::new(schema);
        cache
            .write()
            .expect("MCP input schema cache lock poisoned")
            .insert(TypeId::of::<T>(), schema.clone());
        schema
    })
}

fn expose_union_types(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for child in object.values_mut() {
                expose_union_types(child);
            }
            if object.contains_key("type") {
                return;
            }
            for keyword in ["oneOf", "anyOf"] {
                let Some(variants) = object.get(keyword).and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                let mut types = Vec::new();
                for variant in variants {
                    let Some(declared) = declared_types(variant) else {
                        types.clear();
                        break;
                    };
                    for declared_type in declared {
                        if !types.contains(&declared_type) {
                            types.push(declared_type);
                        }
                    }
                }
                if !types.is_empty() {
                    object.insert(
                        "type".to_string(),
                        if types.len() == 1 {
                            serde_json::Value::String(types.remove(0))
                        } else {
                            serde_json::Value::Array(
                                types.into_iter().map(serde_json::Value::String).collect(),
                            )
                        },
                    );
                    return;
                }
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                expose_union_types(child);
            }
        }
        _ => {}
    }
}

fn declared_types(value: &serde_json::Value) -> Option<Vec<String>> {
    match value.get("type")? {
        serde_json::Value::String(value) => Some(vec![value.clone()]),
        serde_json::Value::Array(values) => values
            .iter()
            .map(|value| value.as_str().map(str::to_string))
            .collect(),
        _ => None,
    }
}

/// Returns the canonical input contract for a tool with no arguments.
pub fn mcp_empty_input_schema() -> Arc<JsonObject> {
    static SCHEMA: OnceLock<Arc<JsonObject>> = OnceLock::new();
    SCHEMA
        .get_or_init(|| {
            Arc::new(
                serde_json::json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                })
                .as_object()
                .expect("empty MCP input schema must be an object")
                .clone(),
            )
        })
        .clone()
}

fn validate_client_facing_input<T: Any>(schema: &JsonObject) {
    assert_eq!(
        schema.get("$schema").and_then(serde_json::Value::as_str),
        Some("https://json-schema.org/draft/2020-12/schema"),
        "MCP input schema for {} must declare JSON Schema 2020-12",
        std::any::type_name::<T>()
    );
    assert_eq!(
        schema.get("type").and_then(serde_json::Value::as_str),
        Some("object"),
        "MCP input schema for {} must have an object root",
        std::any::type_name::<T>()
    );
    assert!(
        !contains_reference(&serde_json::Value::Object(schema.clone())),
        "MCP input schema for {} must be self-contained",
        std::any::type_name::<T>()
    );
    let schema = serde_json::Value::Object(schema.clone());
    validate_property_types::<T>(&schema, "$".to_string());
    validate_object_unions::<T>(&schema, "$".to_string());
}

fn validate_property_types<T: Any>(value: &serde_json::Value, path: String) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(properties) = object
                .get("properties")
                .and_then(serde_json::Value::as_object)
            {
                for (name, property) in properties {
                    assert!(
                        property.get("type").is_some(),
                        "MCP input schema for {} must expose the JSON type at {path}/properties/{name}: {property}",
                        std::any::type_name::<T>()
                    );
                }
            }
            for (name, child) in object {
                // A schema's `properties` member maps field names to schemas
                // and is not a schema itself; descend into each field schema
                // so a field literally named `properties` (for example
                // GeoJSON feature properties) is not misread as a schema
                // node.
                if name == "properties"
                    && let Some(fields) = child.as_object()
                {
                    for (field, field_schema) in fields {
                        validate_property_types::<T>(
                            field_schema,
                            format!("{path}/properties/{field}"),
                        );
                    }
                    continue;
                }
                validate_property_types::<T>(child, format!("{path}/{name}"));
            }
        }
        serde_json::Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                validate_property_types::<T>(child, format!("{path}/{index}"));
            }
        }
        _ => {}
    }
}

fn validate_object_unions<T: Any>(value: &serde_json::Value, path: String) {
    match value {
        serde_json::Value::Object(object) => {
            for union_keyword in ["oneOf", "anyOf"] {
                if let Some(variants) = object
                    .get(union_keyword)
                    .and_then(serde_json::Value::as_array)
                    && !variants.is_empty()
                    && variants.iter().all(schema_is_object)
                {
                    assert!(
                        schema_accepts_object(object.get("type")),
                        "MCP input schema for {} must expose object type at {path}: {}",
                        std::any::type_name::<T>(),
                        serde_json::Value::Object(object.clone())
                    );
                }
            }
            for (name, child) in object {
                validate_object_unions::<T>(child, format!("{path}/{name}"));
            }
        }
        serde_json::Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                validate_object_unions::<T>(child, format!("{path}/{index}"));
            }
        }
        _ => {}
    }
}

fn schema_is_object(value: &serde_json::Value) -> bool {
    value
        .as_object()
        .is_some_and(|schema| schema_accepts_object(schema.get("type")))
}

fn schema_accepts_object(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::String(value)) => value == "object",
        Some(serde_json::Value::Array(values)) => values.iter().any(|value| value == "object"),
        _ => false,
    }
}

fn contains_reference(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => {
            object.contains_key("$ref") || object.values().any(contains_reference)
        }
        serde_json::Value::Array(values) => values.iter().any(contains_reference),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[allow(dead_code)]
    #[derive(Deserialize, JsonSchema)]
    struct Nested {
        value: String,
    }

    #[allow(dead_code)]
    #[derive(Deserialize, JsonSchema)]
    struct Request {
        nested: Nested,
    }

    #[allow(dead_code)]
    #[derive(Deserialize, JsonSchema)]
    #[serde(untagged)]
    enum ScalarValue {
        Text(String),
        Number(i64),
    }

    #[allow(dead_code)]
    #[derive(Deserialize, JsonSchema)]
    struct UnionRequest {
        optional_nested: Option<Nested>,
        value: ScalarValue,
    }

    #[test]
    fn canonical_input_schema_is_2020_12_and_self_contained() {
        let schema = mcp_input_schema::<Request>();
        let schema_value = serde_json::Value::Object(schema.as_ref().clone());
        jsonschema::meta::validate(&schema_value).expect("schema must satisfy its meta-schema");
        assert_eq!(
            schema.get("$schema").and_then(serde_json::Value::as_str),
            Some("https://json-schema.org/draft/2020-12/schema")
        );
        assert_eq!(
            schema_value
                .pointer("/properties/nested/type")
                .and_then(serde_json::Value::as_str),
            Some("object")
        );
        assert!(!contains_reference(&schema_value));
    }

    #[allow(dead_code)]
    #[derive(Deserialize, JsonSchema)]
    struct FeatureLikeRequest {
        name: String,
        properties: std::collections::BTreeMap<String, i64>,
    }

    #[test]
    fn canonical_input_schema_accepts_a_field_named_properties() {
        // GeoJSON-style payloads carry a field literally named `properties`;
        // the validator must treat its schema as one field schema, not as a
        // schema `properties` map.
        let schema = mcp_input_schema::<FeatureLikeRequest>();
        let schema_value = serde_json::Value::Object(schema.as_ref().clone());
        jsonschema::meta::validate(&schema_value).expect("schema must satisfy its meta-schema");
        assert_eq!(
            schema_value
                .pointer("/properties/properties/type")
                .and_then(serde_json::Value::as_str),
            Some("object")
        );
    }

    #[test]
    fn empty_input_schema_is_explicit_and_closed() {
        assert_eq!(
            mcp_empty_input_schema().as_ref(),
            serde_json::json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {},
                "additionalProperties": false
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn canonical_input_schema_exposes_union_branch_types() {
        let schema = mcp_input_schema::<UnionRequest>();
        jsonschema::meta::validate(&serde_json::Value::Object(schema.as_ref().clone()))
            .expect("schema must satisfy its meta-schema");
        assert_eq!(
            schema
                .get("properties")
                .and_then(serde_json::Value::as_object)
                .and_then(|properties| properties.get("optional_nested"))
                .and_then(|property| property.get("type")),
            Some(&serde_json::json!(["object", "null"]))
        );
        assert_eq!(
            schema
                .get("properties")
                .and_then(serde_json::Value::as_object)
                .and_then(|properties| properties.get("value"))
                .and_then(|property| property.get("type")),
            Some(&serde_json::json!(["string", "integer"]))
        );
    }
}
