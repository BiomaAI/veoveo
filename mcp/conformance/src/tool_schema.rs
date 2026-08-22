use anyhow::{Result, anyhow, bail, ensure};
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_SCHEMA_BYTES: usize = 1024 * 1024;
pub const MAX_SCHEMA_DEPTH: usize = 64;
pub const MAX_SCHEMA_NODES: usize = 50_000;
pub const MAX_SCHEMA_REFERENCES: usize = 4_096;
pub const MAX_SCHEMA_BRANCHES: usize = 4_096;

/// Resource bounds observed while validating one JSON Schema document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaStats {
    pub serialized_bytes: usize,
    pub maximum_depth: usize,
    pub nodes: usize,
    pub references: usize,
    pub composition_branches: usize,
}

#[derive(Default)]
struct SchemaInspection {
    maximum_depth: usize,
    nodes: usize,
    references: usize,
    composition_branches: usize,
}

/// Validate one MCP tool input through the bounded JSON Schema 2020-12 profile.
pub fn validate_tool_input_schema(tool: &Tool) -> Result<SchemaStats> {
    let schema = Value::Object(tool.input_schema.as_ref().clone());
    let serialized_bytes = serde_json::to_vec(&schema)?.len();
    ensure!(
        serialized_bytes <= MAX_SCHEMA_BYTES,
        "tool `{}` input schema is {} bytes; maximum is {}",
        tool.name,
        serialized_bytes,
        MAX_SCHEMA_BYTES
    );

    let mut inspection = SchemaInspection::default();
    inspect_schema(&schema, 0, &mut inspection).map_err(|error| {
        anyhow!(
            "tool `{}` input schema exceeds the bounded profile: {error}",
            tool.name
        )
    })?;
    jsonschema::meta::validate(&schema)
        .map_err(|error| anyhow!("tool `{}` input schema is invalid: {error}", tool.name))?;
    ensure!(
        schema_accepts_object(schema.get("type")),
        "tool `{}` input schema root must declare object type: {schema}",
        tool.name
    );

    Ok(SchemaStats {
        serialized_bytes,
        maximum_depth: inspection.maximum_depth,
        nodes: inspection.nodes,
        references: inspection.references,
        composition_branches: inspection.composition_branches,
    })
}

fn inspect_schema(value: &Value, depth: usize, inspection: &mut SchemaInspection) -> Result<()> {
    if depth > MAX_SCHEMA_DEPTH {
        bail!("nesting depth exceeds {MAX_SCHEMA_DEPTH}");
    }
    inspection.maximum_depth = inspection.maximum_depth.max(depth);
    inspection.nodes += 1;
    if inspection.nodes > MAX_SCHEMA_NODES {
        bail!("node count exceeds {MAX_SCHEMA_NODES}");
    }

    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref") {
                let reference = reference
                    .as_str()
                    .ok_or_else(|| anyhow!("$ref must be a string"))?;
                ensure!(
                    reference.starts_with('#'),
                    "external schema reference `{reference}` is forbidden"
                );
                inspection.references += 1;
                if inspection.references > MAX_SCHEMA_REFERENCES {
                    bail!("reference count exceeds {MAX_SCHEMA_REFERENCES}");
                }
            }

            for keyword in ["allOf", "anyOf", "oneOf"] {
                if let Some(branches) = object.get(keyword).and_then(Value::as_array) {
                    inspection.composition_branches += branches.len();
                    if inspection.composition_branches > MAX_SCHEMA_BRANCHES {
                        bail!("composition branch count exceeds {MAX_SCHEMA_BRANCHES}");
                    }
                }
            }

            for child in object.values() {
                inspect_schema(child, depth + 1, inspection)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                inspect_schema(child, depth + 1, inspection)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn schema_accepts_object(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(value)) => value == "object",
        Some(Value::Array(values)) => values.iter().any(|value| value == "object"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rmcp::model::Tool;
    use serde_json::json;

    use super::*;

    fn tool(schema: Value) -> Tool {
        Tool::new(
            "bounded",
            "bounded schema",
            Arc::new(schema.as_object().expect("object schema").clone()),
        )
    }

    #[test]
    fn accepts_schemars_same_document_references_and_composition() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {
                "ArtifactId": { "type": "string" }
            },
            "properties": {
                "artifact_id": { "$ref": "#/$defs/ArtifactId" },
                "mode": {
                    "oneOf": [
                        { "type": "string", "const": "read" },
                        { "type": "string", "const": "write" }
                    ]
                }
            },
            "required": ["artifact_id"]
        });

        let stats = validate_tool_input_schema(&tool(schema)).unwrap();
        assert_eq!(stats.references, 1);
        assert_eq!(stats.composition_branches, 2);
    }

    #[test]
    fn rejects_external_references_without_fetching_them() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "payload": { "$ref": "https://example.invalid/schema.json" }
            }
        });

        let error = validate_tool_input_schema(&tool(schema)).unwrap_err();
        assert!(error.to_string().contains("external schema reference"));
    }

    #[test]
    fn rejects_an_input_root_that_does_not_declare_object_type() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "array",
            "items": { "type": "string" }
        });

        let error = validate_tool_input_schema(&tool(schema)).unwrap_err();
        assert!(error.to_string().contains("must declare object type"));
    }

    #[test]
    fn rejects_excessive_nesting_before_meta_validation() {
        let mut nested = json!({ "type": "string" });
        for _ in 0..=MAX_SCHEMA_DEPTH {
            nested = json!({ "allOf": [nested] });
        }
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": { "payload": nested }
        });

        let error = validate_tool_input_schema(&tool(schema)).unwrap_err();
        assert!(error.to_string().contains("nesting depth exceeds"));
    }
}
