//! Local map/key agreement for the supported maintained-generator forms.
use super::*;
use serde_json::Map;

#[derive(Default)]
struct KeyFacts {
    finite: Option<BTreeSet<String>>,
    pattern: Option<String>,
    min_length: Option<u64>,
    max_length: Option<u64>,
}
fn key_facts(key: &Value, root: NamingSchemaContext<'_>) -> Result<KeyFacts, NamingProfileError> {
    let selected = inherited_profile(key, root, &mut BTreeSet::new(), 0)?;
    if selected
        .as_ref()
        .is_some_and(|p| !matches!(p.role(), NamingRole::Scalar { .. }))
    {
        return Err(NamingProfileError::KeySchema);
    }
    if instance_shape(key, root, selected.as_ref(), &mut BTreeSet::new(), 0)? != STRING {
        return Err(NamingProfileError::KeySchema);
    }
    let mut facts = KeyFacts::default();
    for node in same_instance_nodes(key, root, selected.as_ref())? {
        if node.contains_key("anyOf") || node.contains_key("oneOf") {
            return Err(NamingProfileError::KeySchema);
        }
        if let Some(values) = node.get("enum") {
            let values = values
                .as_array()
                .filter(|v| !v.is_empty())
                .ok_or(NamingProfileError::KeySchema)?;
            let keys: BTreeSet<String> = values
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or(NamingProfileError::KeySchema)
                })
                .collect::<Result<_, _>>()?;
            if keys.len() != values.len() {
                return Err(NamingProfileError::KeySchema);
            }
            intersect(&mut facts.finite, keys);
        }
        if let Some(value) = node.get("const") {
            intersect(
                &mut facts.finite,
                [value
                    .as_str()
                    .ok_or(NamingProfileError::KeySchema)?
                    .to_owned()]
                .into(),
            );
        }
        if let Some(pattern) = node.get("pattern") {
            let pattern = pattern.as_str().ok_or(NamingProfileError::KeySchema)?;
            if facts.pattern.as_ref().is_some_and(|p| p != pattern) {
                return Err(NamingProfileError::KeySchema);
            }
            facts.pattern = Some(pattern.to_owned());
        }
        for (keyword, lower) in [("minLength", true), ("maxLength", false)] {
            if let Some(value) = node.get(keyword) {
                let value = value.as_u64().ok_or(NamingProfileError::KeySchema)?;
                if lower {
                    facts.min_length = Some(facts.min_length.map_or(value, |old| old.max(value)));
                } else {
                    facts.max_length = Some(facts.max_length.map_or(value, |old| old.min(value)));
                }
            }
        }
    }
    // Map emission chooses a pattern before enum; a mixed claim needs a different explicit profile.
    if facts
        .min_length
        .zip(facts.max_length)
        .is_some_and(|(min, max)| min > max)
        || facts.finite.as_ref().is_some_and(BTreeSet::is_empty)
        || facts.finite.is_some() && facts.pattern.is_some()
    {
        return Err(NamingProfileError::KeySchema);
    }
    Ok(facts)
}
fn intersect(current: &mut Option<BTreeSet<String>>, values: BTreeSet<String>) {
    *current = Some(match current.take() {
        Some(previous) => previous.intersection(&values).cloned().collect(),
        None => values,
    });
}
/// Follow only local same-instance references/allOf, preserving constraint and marker siblings.
fn same_instance_nodes<'a>(
    node: &'a Value,
    root: NamingSchemaContext<'a>,
    selected: Option<&NamingProfile>,
) -> Result<Vec<&'a Map<String, Value>>, NamingProfileError> {
    fn visit<'a>(
        node: &'a Value,
        root: NamingSchemaContext<'a>,
        selected: Option<&NamingProfile>,
        refs: &mut BTreeSet<String>,
        depth: usize,
        nodes: &mut Vec<&'a Map<String, Value>>,
    ) -> Result<(), NamingProfileError> {
        if depth >= 64 {
            return Err(NamingProfileError::NodeKind);
        }
        reject_unsupported_refs(node, root)?;
        let object = node.as_object().ok_or(NamingProfileError::NodeKind)?;
        if let Some(marker) = object.get(NAMING_PROFILE_KEY) {
            let marker: NamingProfile =
                serde_json::from_value(marker.clone()).map_err(|_| NamingProfileError::Metadata)?;
            if selected.is_some_and(|p| p != &marker) {
                return Err(NamingProfileError::Conflict);
            }
        }
        if object.keys().any(|k| {
            matches!(
                k.as_str(),
                "anyOf"
                    | "oneOf"
                    | "not"
                    | "if"
                    | "then"
                    | "else"
                    | "dependentSchemas"
                    | "dependencies"
                    | "unevaluatedProperties"
            )
        }) {
            return Err(NamingProfileError::NodeKind);
        }
        nodes.push(object);
        if let Some(reference) = object.get("$ref") {
            let reference = reference
                .as_str()
                .filter(|r| r.starts_with("#/"))
                .ok_or(NamingProfileError::NodeKind)?;
            if !refs.insert(reference.to_owned()) {
                return Err(NamingProfileError::NodeKind);
            }
            visit(
                local_target(root, reference)?,
                root,
                selected,
                refs,
                depth + 1,
                nodes,
            )?;
            refs.remove(reference);
        }
        if let Some(branches) = object.get("allOf") {
            for branch in branches
                .as_array()
                .filter(|v| !v.is_empty())
                .ok_or(NamingProfileError::NodeKind)?
            {
                visit(branch, root, selected, refs, depth + 1, nodes)?;
            }
        }
        Ok(())
    }
    let mut nodes = Vec::new();
    visit(node, root, selected, &mut BTreeSet::new(), 0, &mut nodes)?;
    Ok(nodes)
}
fn schema_route(value: &Value) -> Result<(), NamingProfileError> {
    Schema::try_from(value.clone())
        .map(|_| ())
        .map_err(|_| NamingProfileError::DictionaryKeys)
}
pub(super) fn validate(
    node: &Value,
    root: NamingSchemaContext<'_>,
    selected: &NamingProfile,
    key: &Value,
) -> Result<(), NamingProfileError> {
    if instance_shape(node, root, Some(selected), &mut BTreeSet::new(), 0)? != OBJECT {
        return Err(NamingProfileError::NodeKind);
    }
    let keys = key_facts(key, root)?;
    let mut properties = None;
    let mut patterns = None;
    let mut additional = None;
    let mut property_names = Vec::new();
    let mut mapped_value = None;
    for object in same_instance_nodes(node, root, Some(selected))? {
        if object.contains_key("const") || object.contains_key("enum") {
            return Err(NamingProfileError::DictionaryKeys);
        }
        for (keyword, slot) in [
            ("properties", &mut properties),
            ("patternProperties", &mut patterns),
        ] {
            if let Some(value) = object.get(keyword) {
                let value = value
                    .as_object()
                    .ok_or(NamingProfileError::DictionaryKeys)?;
                if slot.as_ref().is_some_and(|p| *p != value) {
                    return Err(NamingProfileError::DictionaryKeys);
                }
                for route in value.values() {
                    schema_route(route)?;
                    if mapped_value.is_some_and(|previous| previous != route) {
                        return Err(NamingProfileError::DictionaryKeys);
                    }
                    mapped_value = Some(route);
                }
                *slot = Some(value);
            }
        }
        if let Some(value) = object.get("additionalProperties") {
            schema_route(value)?;
            if additional.is_some_and(|p| p != value) {
                return Err(NamingProfileError::DictionaryKeys);
            }
            additional = Some(value);
        }
        if let Some(value) = object.get("propertyNames") {
            property_names.push(key_facts(value, root)?);
        }
        if let Some(required) = object.get("required") {
            if !required.as_array().is_some_and(|v| v.is_empty()) {
                return Err(NamingProfileError::DictionaryKeys);
            }
        }
    }
    let properties: BTreeSet<String> = properties
        .into_iter()
        .flat_map(|p| p.keys().cloned())
        .collect();
    let patterns: BTreeSet<String> = patterns
        .into_iter()
        .flat_map(|p| p.keys().cloned())
        .collect();
    match (&keys.finite, &keys.pattern) {
        (Some(finite), None)
            if properties == *finite
                && patterns.is_empty()
                && additional == Some(&Value::Bool(false)) => {}
        (None, Some(pattern))
            if properties.is_empty()
                && patterns == BTreeSet::from([pattern.clone()])
                && additional == Some(&Value::Bool(false)) => {}
        (None, None)
            if properties.is_empty()
                && patterns.is_empty()
                && additional.is_some_and(|v| v != &Value::Bool(false)) => {}
        _ => return Err(NamingProfileError::DictionaryKeys),
    }
    for restriction in property_names {
        if restriction
            .min_length
            .is_some_and(|bound| keys.min_length != Some(bound))
            || restriction
                .max_length
                .is_some_and(|bound| keys.max_length != Some(bound))
            || restriction
                .finite
                .as_ref()
                .is_some_and(|finite| keys.finite.as_ref() != Some(finite))
            || restriction
                .pattern
                .as_ref()
                .is_some_and(|pattern| keys.pattern.as_ref() != Some(pattern))
        {
            return Err(NamingProfileError::DictionaryKeys);
        }
    }
    Ok(())
}
