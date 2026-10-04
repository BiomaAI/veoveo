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
    inspect_keywords(&schema, &mut inspection, true)?;
    jsonschema::meta::validate(&schema)
        .map_err(|error| anyhow!("tool `{}` input schema is invalid: {error}", tool.name))?;
    ensure!(
        schema_accepts_object(schema.get("type")),
        "tool `{}` input schema root must declare object type: {schema}",
        tool.name
    );

    inspect_closed_shapes(&schema, &schema, "#", false, &mut Vec::new(), &mut 0).map_err(
        |error| {
            anyhow!(
                "tool `{}` input schema has an open controlled shape: {error}",
                tool.name
            )
        },
    )?;

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

// Schema-bearing keywords only. Annotation and instance values (including
// examples, defaults and enum members) are ordinary JSON, never schemas.
fn schema_children(schema: &Value, definitions: bool) -> Vec<(String, &Value)> {
    let mut children = Vec::new();
    for keyword in [
        "properties",
        "patternProperties",
        "dependentSchemas",
        "$defs",
        "definitions",
    ] {
        if !definitions && matches!(keyword, "$defs" | "definitions") {
            continue;
        }
        if let Some(map) = schema.get(keyword).and_then(Value::as_object) {
            children.extend(
                map.iter()
                    .map(|(name, child)| (format!("{keyword}/{name}"), child)),
            );
        }
    }
    for keyword in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(array) = schema.get(keyword).and_then(Value::as_array) {
            children.extend(
                array
                    .iter()
                    .enumerate()
                    .map(|(i, child)| (format!("{keyword}/{i}"), child)),
            );
        }
    }
    for keyword in [
        "items",
        "additionalProperties",
        "unevaluatedProperties",
        "unevaluatedItems",
        "contains",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
        "contentSchema",
    ] {
        if let Some(child) = schema.get(keyword) {
            children.push((keyword.into(), child));
        }
    }
    children
}

fn inspect_keywords(
    schema: &Value,
    inspection: &mut SchemaInspection,
    is_root: bool,
) -> Result<()> {
    ensure!(
        is_root || schema.get("$id").is_none(),
        "nested schema $id changes the local reference base and is outside the supported profile"
    );
    ensure!(
        schema.get("$dynamicRef").is_none() && schema.get("$dynamicAnchor").is_none(),
        "dynamic schema references are outside the supported local $ref profile"
    );
    if let Some(reference) = schema.get("$ref") {
        let reference = reference
            .as_str()
            .ok_or_else(|| anyhow!("$ref must be a string"))?;
        ensure!(
            reference.starts_with('#'),
            "external schema reference `{reference}` is forbidden"
        );
        inspection.references += 1;
        ensure!(
            inspection.references <= MAX_SCHEMA_REFERENCES,
            "reference count exceeds {MAX_SCHEMA_REFERENCES}"
        );
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            inspection.composition_branches += branches.len();
            ensure!(
                inspection.composition_branches <= MAX_SCHEMA_BRANCHES,
                "composition branch count exceeds {MAX_SCHEMA_BRANCHES}"
            );
        }
    }
    for (_, child) in schema_children(schema, true) {
        inspect_keywords(child, inspection, false)?;
    }
    Ok(())
}

fn local_reference<'a>(root: &'a Value, reference: &str) -> Result<&'a Value> {
    let fragment = reference
        .strip_prefix('#')
        .ok_or_else(|| anyhow!("external schema reference `{reference}` is forbidden"))?;
    if fragment.is_empty() || fragment.starts_with('/') {
        return root
            .pointer(fragment)
            .ok_or_else(|| anyhow!("unresolved local schema reference `{reference}`"));
    }
    fn anchor<'a>(schema: &'a Value, name: &str) -> Option<&'a Value> {
        if schema.get("$anchor").and_then(Value::as_str) == Some(name) {
            return Some(schema);
        }
        schema_children(schema, true)
            .into_iter()
            .find_map(|(_, child)| anchor(child, name))
    }
    anchor(root, fragment).ok_or_else(|| anyhow!("unresolved local schema reference `{reference}`"))
}

fn count_closure_visit(visits: &mut usize) -> Result<()> {
    *visits += 1;
    ensure!(
        *visits <= MAX_SCHEMA_NODES,
        "closure traversal count exceeds {MAX_SCHEMA_NODES}"
    );
    Ok(())
}

fn closes_object(
    schema: &Value,
    root: &Value,
    references: &mut Vec<String>,
    visits: &mut usize,
) -> Result<bool> {
    count_closure_visit(visits)?;
    if schema == &Value::Bool(false)
        || schema.get("additionalProperties") == Some(&Value::Bool(false))
        || schema.get("unevaluatedProperties") == Some(&Value::Bool(false))
    {
        return Ok(true);
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        && !references.iter().any(|seen| seen == reference)
    {
        ensure!(
            references.len() < MAX_SCHEMA_DEPTH,
            "reference nesting depth exceeds {MAX_SCHEMA_DEPTH}"
        );
        references.push(reference.into());
        let closed = closes_object(local_reference(root, reference)?, root, references, visits)?;
        references.pop();
        if closed {
            return Ok(true);
        }
    }
    if let Some(branches) = schema.get("allOf").and_then(Value::as_array) {
        for branch in branches {
            if closes_object(branch, root, references, visits)? {
                return Ok(true);
            }
        }
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            let mut closed = !branches.is_empty();
            for branch in branches {
                // Nonobject alternatives need no object closure.
                let nonobject =
                    branch.get("type").is_some() && !schema_accepts_object(branch.get("type"));
                closed &= nonobject || closes_object(branch, root, references, visits)?;
            }
            if closed {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn inspect_closed_shapes(
    schema: &Value,
    root: &Value,
    path: &str,
    inherited: bool,
    references: &mut Vec<(String, bool)>,
    visits: &mut usize,
) -> Result<()> {
    count_closure_visit(visits)?;
    let closed = inherited || closes_object(schema, root, &mut Vec::new(), visits)?;
    let dictionary = schema.get("properties").is_none()
        && (schema
            .get("additionalProperties")
            .is_some_and(|value| value.is_object() || value == &Value::Bool(true))
            || schema.get("patternProperties").is_some());
    let controlled = schema
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|properties| !properties.is_empty())
        || (schema_accepts_object(schema.get("type")) && !dictionary)
        || path == "#";
    ensure!(
        !controlled || closed,
        "{path} must reject unknown object fields"
    );
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        && !references
            .iter()
            .any(|(seen, inherited)| seen == reference && *inherited == closed)
    {
        ensure!(
            references.len() < MAX_SCHEMA_DEPTH,
            "reference nesting depth exceeds {MAX_SCHEMA_DEPTH}"
        );
        references.push((reference.into(), closed));
        inspect_closed_shapes(
            local_reference(root, reference)?,
            root,
            reference,
            closed,
            references,
            visits,
        )?;
        references.pop();
    }
    for (keyword, child) in schema_children(schema, false) {
        if matches!(keyword.as_str(), "not" | "if" | "propertyNames") {
            continue;
        }
        // Same-instance composition inherits closure. Property values and array
        // elements have their own object shape and cannot inherit it.
        let same_instance = matches!(keyword.as_str(), "then" | "else")
            || keyword.starts_with("dependentSchemas/")
            || ["allOf/", "anyOf/", "oneOf/"]
                .iter()
                .any(|prefix| keyword.starts_with(prefix));
        inspect_closed_shapes(
            child,
            root,
            &format!("{path}/{keyword}"),
            closed && same_instance,
            references,
            visits,
        )?;
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
            "additionalProperties": false,
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
    #[test]
    fn rejects_open_root_and_reachable_nested_objects() {
        for schema in [
            json!({"type":"object", "properties":{"name":{"type":"string"}}}),
            json!({"type":"object", "additionalProperties":false, "properties":{
                "child":{"$ref":"#/$defs/Child"}}, "$defs":{
                "Child":{"type":"object", "properties":{"name":{"type":"string"}}}}}),
            json!({"type":"object", "additionalProperties":false, "properties":{
                "children":{"type":"array", "items":{"type":"object", "properties":{"name":{"type":"string"}}}}}}),
        ] {
            let error = validate_tool_input_schema(&tool(schema)).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("must reject unknown object fields"),
                "{error}"
            );
        }
    }

    #[test]
    fn accepts_closed_union_branches_without_a_root_closure_keyword() {
        for keyword in ["oneOf", "anyOf", "allOf"] {
            let mut schema = json!({"type":"object"});
            schema[keyword] = json!([
                {"type":"object", "properties":{"kind":{"const":"unit"}}, "additionalProperties":false},
                {"type":"object", "properties":{"kind":{"const":"data"}, "value":{"type":"string"}}, "additionalProperties":false}
            ]);
            validate_tool_input_schema(&tool(schema)).unwrap();
        }
        let schema = json!({"type":"object", "oneOf":[
            {"type":"object", "properties":{"kind":{"const":"closed"}}, "additionalProperties":false},
            {"type":"object", "properties":{"kind":{"const":"open"}}}
        ]});
        assert!(validate_tool_input_schema(&tool(schema)).is_err());
    }

    #[test]
    fn keeps_typed_dictionaries_and_opaque_payloads_open() {
        let schema = json!({"type":"object", "additionalProperties":false, "properties":{
            "counts":{"type":"object", "additionalProperties":{"type":"integer"}},
            "provider":{"type":"object", "additionalProperties":true},
            "payload":true,
            "supplied_schema":{}
        }, "$defs":{"Unreachable":{"type":"object", "properties":{"open":{"type":"string"}}}}});
        validate_tool_input_schema(&tool(schema)).unwrap();
    }

    #[test]
    fn schema_keywords_do_not_include_annotation_payloads_or_property_names() {
        let schema = json!({"type":"object", "additionalProperties":false,
            "properties":{"$ref":{"type":"string"}, "allOf":{"type":"string"}},
            "examples":[{"$ref":"https://example.invalid/data", "allOf":[{},{}]}],
            "default":{"$ref":"https://example.invalid/default"},
            "enum":[{"$ref":"https://example.invalid/enum"}],
            "const":{"$ref":"https://example.invalid/const"}
        });
        let stats = validate_tool_input_schema(&tool(schema)).unwrap();
        assert_eq!(stats.references, 0);
        assert_eq!(stats.composition_branches, 0);
    }

    #[test]
    fn rejects_external_references_in_unused_definitions() {
        let schema = json!({"type":"object", "additionalProperties":false, "$defs":{
            "Unused":{"$ref":"https://example.invalid/schema"}
        }});
        assert!(
            validate_tool_input_schema(&tool(schema))
                .unwrap_err()
                .to_string()
                .contains("external schema reference")
        );
    }

    #[test]
    fn follows_recursive_local_references_without_revisiting_forever() {
        let schema = json!({"type":"object", "additionalProperties":false, "properties":{
            "next":{"$ref":"#"}
        }});
        validate_tool_input_schema(&tool(schema)).unwrap();
        let schema = json!({"type":"object", "additionalProperties":false, "properties":{
            "next":{"$ref":"#/$defs/Missing"}
        }});
        assert!(
            validate_tool_input_schema(&tool(schema))
                .unwrap_err()
                .to_string()
                .contains("unresolved local schema reference")
        );
    }

    #[test]
    fn actual_media_and_frames_owner_schemas_pass_the_shared_profile() {
        for schema in [
            serde_json::to_value(schemars::schema_for!(veoveo_media_mcp::contract::RunArgs))
                .unwrap(),
            serde_json::to_value(schemars::schema_for!(
                veoveo_frames_mcp::contract::BatchTransformRequest
            ))
            .unwrap(),
            serde_json::to_value(schemars::schema_for!(
                veoveo_frames_mcp::contract::PublishWorldRequest
            ))
            .unwrap(),
        ] {
            validate_tool_input_schema(&tool(schema)).unwrap();
        }
    }
    #[test]
    fn bounds_repeated_reference_expansion_including_closure_proofs() {
        let mut definitions = serde_json::Map::new();
        definitions.insert(
            "N0".into(),
            json!({"type":"object", "additionalProperties":false}),
        );
        for i in 1..40 {
            let reference = format!("#/$defs/N{}", i - 1);
            definitions.insert(
                format!("N{i}"),
                json!({"anyOf":[{"$ref":reference}, {"$ref":reference}]}),
            );
        }
        let schema = json!({"type":"object", "additionalProperties":false,
            "properties":{"node":{"$ref":"#/$defs/N39"}}, "$defs":definitions});
        let error = validate_tool_input_schema(&tool(schema)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("closure traversal count exceeds"),
            "{error}"
        );
    }

    #[test]
    fn constraint_schemas_do_not_describe_decoded_objects() {
        let schema = json!({"type":"object", "unevaluatedProperties":false,
            "properties":{"kind":{"type":"string"}},
            "not":{"properties":{"kind":{"const":"blocked"}}},
            "if":{"properties":{"kind":{"const":"extended"}}},
            "then":{"properties":{"extra":{"type":"integer"}}},
            "else":{"properties":{"other":{"type":"boolean"}}},
            "dependentSchemas":{"kind":{"properties":{"another":{"type":"integer"}}}},
            "propertyNames":{"not":{"properties":{"constraint":{"type":"string"}}}}
        });
        validate_tool_input_schema(&tool(schema)).unwrap();
    }

    #[test]
    fn rejects_dynamic_references_outside_the_supported_profile() {
        for reference in ["#node", "https://example.invalid/schema"] {
            let schema = json!({"type":"object", "additionalProperties":false,
                "properties":{"child":{"$dynamicRef":reference}}});
            let error = validate_tool_input_schema(&tool(schema)).unwrap_err();
            assert!(error.to_string().contains("dynamic schema references"));
        }
    }

    #[test]
    fn recursive_targets_do_not_inherit_the_outer_instances_closure() {
        let schema = json!({"type":"object", "unevaluatedProperties":false,
        "$ref":"#/$defs/Node", "$defs":{"Node":{"type":"object", "properties":{
            "next":{"$ref":"#/$defs/Node"}
        }}}});
        let error = validate_tool_input_schema(&tool(schema)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("must reject unknown object fields")
        );
    }
    #[test]
    fn nested_schema_ids_cannot_redirect_local_reference_closure() {
        let schema = json!({
            "type":"object", "additionalProperties":false,
            "$defs":{"T":{"type":"object", "additionalProperties":false}},
            "properties":{"child":{
                "$id":"https://example.invalid/child", "$ref":"#/$defs/T",
                "$defs":{"T":{"type":"object", "properties":{"name":{"type":"string"}}}}
            }}
        });
        let error = validate_tool_input_schema(&tool(schema)).unwrap_err();
        assert!(error.to_string().contains("nested schema $id"), "{error}");
    }

    #[test]
    fn root_schema_id_keeps_same_document_reference_resolution() {
        let schema = json!({
            "$id":"https://example.invalid/root", "type":"object", "additionalProperties":false,
            "$defs":{"T":{"type":"object", "additionalProperties":false,
                "properties":{"name":{"type":"string"}}}},
            "properties":{"child":{"$ref":"#/$defs/T"}}
        });
        validate_tool_input_schema(&tool(schema)).unwrap();
    }
}
