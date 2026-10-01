//! Detail payloads may contain reviewed identifiers, timestamps and closed vocabularies.
use serde_json::Value;
use std::collections::BTreeSet;
use veoveo_audit_contract::AuditDetail;

// Adding an identifier here requires reviewing its validation and disclosure surface.
const IDENTIFIERS: &[&str] = &[
    "Sha256Digest",
    "ResourceUri",
    "TaskId",
    "PrincipalId",
    "GroupId",
    "RoleId",
    // Server-owned observations carry reviewed provenance, never resource bodies.
    "CollectionId",
    "Revision",
    "TenantId",
    "WorkContextId",
    // Installation profile route id: validated path token, never a credential or URI.
    "GatewayProfileId",
    "DataLabelId",
    "ExternalSystemId",
    "ExternalRecordId",
];
// Sha256Digest deliberately inlines this closed scalar schema instead of a $ref.
const INLINE_IDENTIFIER_PATTERNS: &[&str] = &["^sha256:[0-9a-f]{64}$", "^[0-9a-f]{64}$"];

fn check(
    schema: &Value,
    root: &Value,
    path: &str,
    seen: &mut BTreeSet<String>,
) -> Result<(), String> {
    if schema == &Value::Bool(false) {
        return Ok(());
    }
    let object = schema
        .as_object()
        .ok_or_else(|| format!("untyped audit value at {path}"))?;
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        let name = reference
            .strip_prefix("#/$defs/")
            .ok_or_else(|| format!("nonlocal audit schema at {path}"))?;
        if IDENTIFIERS.contains(&name) || !seen.insert(reference.to_owned()) {
            return Ok(());
        }
        let definition = root
            .pointer(&reference[1..])
            .ok_or_else(|| format!("missing audit schema {reference}"))?;
        return check(definition, root, &format!("{path}/{name}"), seen);
    }
    let is_string = object.get("type").is_some_and(|kind| {
        kind == "string"
            || kind
                .as_array()
                .is_some_and(|types| types.iter().any(|kind| kind == "string"))
    });
    if is_string
        && !object.contains_key("enum")
        && !object.contains_key("const")
        && object.get("format").and_then(Value::as_str) != Some("date-time")
        && !object
            .get("pattern")
            .and_then(Value::as_str)
            .is_some_and(|pattern| INLINE_IDENTIFIER_PATTERNS.contains(&pattern))
    {
        return Err(format!("unreviewed string at {path}"));
    }
    if object
        .get("additionalProperties")
        .is_some_and(|value| value != &Value::Bool(false))
    {
        return Err(format!("open audit object at {path}"));
    }
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for (name, field) in properties {
            check(field, root, &format!("{path}/{name}"), seen)?;
        }
    }
    for keyword in ["oneOf", "anyOf", "allOf", "prefixItems"] {
        if let Some(branches) = object.get(keyword).and_then(Value::as_array) {
            for (index, branch) in branches.iter().enumerate() {
                check(branch, root, &format!("{path}/{keyword}/{index}"), seen)?;
            }
        }
    }
    if let Some(items) = object.get("items") {
        check(items, root, &format!("{path}/items"), seen)?;
    }
    if !object.contains_key("type")
        && !["const", "enum", "oneOf", "anyOf", "allOf"]
            .iter()
            .any(|key| object.contains_key(*key))
    {
        return Err(format!("unconstrained audit value at {path}"));
    }
    Ok(())
}

#[test]
fn every_detail_string_has_a_closed_vocabulary_or_reviewed_identifier() {
    let schema = serde_json::to_value(schemars::schema_for!(AuditDetail)).unwrap();
    check(&schema, &schema, "AuditDetail", &mut BTreeSet::new()).unwrap();
}

#[test]
fn guard_rejects_optional_free_text_open_maps_and_unreviewed_wrappers() {
    for schema in [
        serde_json::json!({"type": ["string", "null"]}),
        serde_json::json!({"type": "string", "pattern": ".*"}),
        serde_json::json!({"type": "object", "additionalProperties": {"type": "string"}}),
        serde_json::json!({"$ref": "#/$defs/UnreviewedId", "$defs": {"UnreviewedId": {"type": "string"}}}),
        serde_json::json!({}),
        Value::Bool(true),
    ] {
        assert!(check(&schema, &schema, "fixture", &mut BTreeSet::new()).is_err());
    }
}
