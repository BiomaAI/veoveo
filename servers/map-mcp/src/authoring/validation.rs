use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::contract::{
    FeatureInput, JSON_FG_CORE_CONFORMANCE, JSON_FG_TYPES_SCHEMAS_CONFORMANCE, LayerStyle,
    MapFeature,
};

const MAX_SCHEMA_BYTES: usize = 256 * 1024;
const MAX_SCHEMA_DEPTH: usize = 32;
const MAX_SCHEMA_NODES: usize = 10_000;
const MAX_PROPERTIES_BYTES: usize = 256 * 1024;
const MAX_PROPERTIES: usize = 256;
const MAX_STYLE_RULES: usize = 32;

#[derive(Clone, Debug)]
pub(super) struct ValidatedSchema {
    pub value: Value,
    pub canonical_json: String,
    pub digest_sha256: String,
}

pub(super) fn validate_schema(schema: &Value) -> Result<ValidatedSchema> {
    let encoded = serde_json::to_vec(schema)?;
    if encoded.len() > MAX_SCHEMA_BYTES {
        bail!("property schema exceeds 256 KiB");
    }
    let object = schema
        .as_object()
        .context("property schema must be a JSON object")?;
    if object.get("type").and_then(Value::as_str) != Some("object") {
        bail!("property schema must declare type object");
    }
    if let Some(dialect) = object.get("$schema").and_then(Value::as_str)
        && dialect != "https://json-schema.org/draft/2020-12/schema"
    {
        bail!("property schema must use JSON Schema 2020-12");
    }
    let mut nodes = 0;
    inspect_schema(schema, 0, &mut nodes)?;
    jsonschema::meta::validate(schema)
        .map_err(|error| anyhow::anyhow!("property schema is invalid: {error}"))?;
    jsonschema::validator_for(schema)
        .map_err(|error| anyhow::anyhow!("property schema cannot be compiled: {error}"))?;
    let canonical_json = canonical_json(schema)?;
    let digest_sha256 = hex_digest(&Sha256::digest(canonical_json.as_bytes()));
    Ok(ValidatedSchema {
        value: schema.clone(),
        canonical_json,
        digest_sha256,
    })
}

pub(super) fn validate_input(schema: &Value, input: &FeatureInput) -> Result<()> {
    input.geometry.validate()?;
    if let Some(time) = &input.time {
        time.validate()?;
    }
    validate_text("semantic_type", &input.semantic_type, 128)?;
    if let Some(title) = &input.title {
        validate_text("title", title, 256)?;
    }
    if input.properties.len() > MAX_PROPERTIES
        || serde_json::to_vec(&input.properties)?.len() > MAX_PROPERTIES_BYTES
    {
        bail!("feature properties exceed 256 entries or 256 KiB");
    }
    for key in input.properties.keys() {
        validate_property_name(key)?;
    }
    validate_resource_uris(&input.related_resources)?;
    validate_resource_uris(&input.evidence_resources)?;
    let instance = serde_json::to_value(&input.properties)?;
    let validator = jsonschema::validator_for(schema)
        .map_err(|error| anyhow::anyhow!("layer property schema cannot be compiled: {error}"))?;
    let errors = validator
        .iter_errors(&instance)
        .take(20)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        bail!(
            "feature properties failed the layer schema: {}",
            errors.join("; ")
        );
    }
    Ok(())
}

pub(super) fn validate_feature(feature: &MapFeature, schema: &Value) -> Result<()> {
    if feature.conforms_to
        != [
            JSON_FG_CORE_CONFORMANCE.to_owned(),
            JSON_FG_TYPES_SCHEMAS_CONFORMANCE.to_owned(),
        ]
    {
        bail!("canonical feature has an invalid JSON-FG conformance declaration");
    }
    validate_input(
        schema,
        &FeatureInput {
            feature_id: Some(feature.id.clone()),
            geometry: feature.geometry.clone(),
            properties: feature.properties.clone(),
            semantic_type: feature.semantic_type.clone(),
            time: feature.time.clone(),
            title: feature.title.clone(),
            related_resources: feature.related_resources.clone(),
            evidence_resources: feature.evidence_resources.clone(),
        },
    )
}

pub(super) fn validate_style(style: &LayerStyle) -> Result<()> {
    if style.rules.len() > MAX_STYLE_RULES {
        bail!("a layer style supports at most 32 rules");
    }
    for rule in &style.rules {
        if let Some(minimum) = rule.minimum_zoom
            && (!minimum.is_finite() || !(0.0..=24.0).contains(&minimum))
        {
            bail!("style minimum_zoom must be within [0, 24]");
        }
        if let Some(maximum) = rule.maximum_zoom
            && (!maximum.is_finite() || !(0.0..=24.0).contains(&maximum))
        {
            bail!("style maximum_zoom must be within [0, 24]");
        }
        if rule
            .minimum_zoom
            .zip(rule.maximum_zoom)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
        {
            bail!("style minimum_zoom must not exceed maximum_zoom");
        }
        for color in [
            rule.fill_color.as_deref(),
            rule.line_color.as_deref(),
            rule.circle_color.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_color(color)?;
        }
        if rule
            .fill_opacity
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            bail!("style fill_opacity must be within [0, 1]");
        }
        for width in [rule.line_width_px, rule.circle_radius_px]
            .into_iter()
            .flatten()
        {
            if !width.is_finite() || !(0.0..=128.0).contains(&width) {
                bail!("style pixel sizes must be within [0, 128]");
            }
        }
        if let Some(property) = &rule.label_property {
            validate_property_name(property)?;
        }
    }
    Ok(())
}

pub(super) fn canonical_json(value: &Value) -> Result<String> {
    let mut output = String::new();
    write_canonical(value, &mut output)?;
    Ok(output)
}

fn inspect_schema(value: &Value, depth: usize, nodes: &mut usize) -> Result<()> {
    if depth > MAX_SCHEMA_DEPTH {
        bail!("property schema exceeds maximum nesting depth 32");
    }
    *nodes += 1;
    if *nodes > MAX_SCHEMA_NODES {
        bail!("property schema exceeds 10000 nodes");
    }
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str)
                && !reference.starts_with('#')
            {
                bail!("property schema contains a remote $ref");
            }
            if let Some(pattern) = object.get("pattern").and_then(Value::as_str)
                && pattern.len() > 256
            {
                bail!("property schema contains a pattern longer than 256 bytes");
            }
            if let Some(properties) = object.get("properties").and_then(Value::as_object)
                && properties.len() > MAX_PROPERTIES
            {
                bail!("property schema declares more than 256 properties");
            }
            for child in object.values() {
                inspect_schema(child, depth + 1, nodes)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                inspect_schema(child, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_text(field: &str, value: &str, maximum: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        bail!("{field} is empty, contains control characters, or exceeds {maximum} bytes");
    }
    Ok(())
}

fn validate_property_name(value: &str) -> Result<()> {
    validate_text("property name", value, 128)?;
    if value.starts_with('$') {
        bail!("feature property names must not start with $");
    }
    Ok(())
}

fn validate_resource_uris(values: &[String]) -> Result<()> {
    if values.len() > 64 {
        bail!("a feature supports at most 64 related or evidence resources");
    }
    for value in values {
        if value.len() > 1024 {
            bail!("feature resource URI exceeds 1024 bytes");
        }
        let uri = url::Url::parse(value).context("feature resource identity must be a URI")?;
        if !matches!(
            uri.scheme(),
            "map" | "artifact" | "recording" | "perception" | "reason" | "time" | "frames" | "view"
        ) {
            bail!("feature resource identities must use a governed Veoveo URI scheme");
        }
    }
    Ok(())
}

fn validate_color(value: &str) -> Result<()> {
    if !matches!(value.len(), 7 | 9)
        || !value.starts_with('#')
        || !value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("style colors must use #RRGGBB or #RRGGBBAA");
    }
    Ok(())
}

fn write_canonical(value: &Value, output: &mut String) -> Result<()> {
    match value {
        Value::Object(object) => {
            output.push('{');
            let sorted = object.iter().collect::<BTreeMap<_, _>>();
            for (index, (key, child)) in sorted.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key)?);
                output.push(':');
                write_canonical(child, output)?;
            }
            output.push('}');
        }
        Value::Array(values) => {
            output.push('[');
            for (index, child) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical(child, output)?;
            }
            output.push(']');
        }
        scalar => output.push_str(&serde_json::to_string(scalar)?),
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_schema_digest_ignores_object_member_order() {
        let first = serde_json::json!({"type":"object","properties":{"b":{"type":"string"},"a":{"type":"integer"}}});
        let second = serde_json::json!({"properties":{"a":{"type":"integer"},"b":{"type":"string"}},"type":"object"});
        assert_eq!(
            validate_schema(&first).unwrap().digest_sha256,
            validate_schema(&second).unwrap().digest_sha256
        );
    }

    #[test]
    fn remote_schema_references_are_rejected() {
        let schema = serde_json::json!({
            "$schema":"https://json-schema.org/draft/2020-12/schema",
            "type":"object",
            "properties":{"x":{"$ref":"https://example.com/schema"}}
        });
        assert!(validate_schema(&schema).is_err());
    }
}
