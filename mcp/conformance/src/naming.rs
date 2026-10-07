//! C33 naming inspection over complete local schema graphs, without remote fetching.
use crate::{
    schema_evidence::{OwnerSchemaEvidence, SchemaEvidenceOrigin},
    tool_schema::{schema_children, validate_schema_document},
};
use anyhow::{Result, anyhow, ensure};
use schemars::Schema;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
use veoveo_types::naming::SpellingCheck;
use veoveo_types::{NamingRole, NamingSchemaContext, ScalarNaming};

mod discovery;
mod literal;
pub(crate) use discovery::check_discovery_since;
pub use discovery::{check_discovery, inspect_discovery};

pub const MAX_NAMING_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_NAMING_WORK: usize = 500_000;
pub const MAX_NAMING_ROOTS: usize = 4096;
pub const NAMING_DEADLINE: Duration = Duration::from_secs(30);

pub fn dto_field_name(name: &str) -> bool {
    let mut chars = name.bytes();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && chars.all(|c| c.is_ascii_alphanumeric())
}
pub fn controlled_name(name: &str) -> bool {
    let mut parts = name.split('_');
    parts.next().is_some_and(|p| {
        p.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            && p.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    }) && parts.all(|p| {
        !p.is_empty()
            && p.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingRootInspection {
    pub location: String,
    pub origin: SchemaEvidenceOrigin,
    pub observations: usize,
    pub source_review_required: bool,
}
/// A single finite budget shared by ALL schemas and owner observations in a run.
pub struct NamingInspection {
    started: Instant,
    bytes: usize,
    work: usize,
    roots: Vec<NamingRootInspection>,
}
impl Default for NamingInspection {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            bytes: 0,
            work: 0,
            roots: vec![],
        }
    }
}
impl NamingInspection {
    pub fn roots(&self) -> &[NamingRootInspection] {
        &self.roots
    }
    fn tick(&mut self) -> Result<()> {
        self.work += 1;
        ensure!(
            self.work <= MAX_NAMING_WORK,
            "naming inspection exceeded aggregate traversal work"
        );
        ensure!(
            self.started.elapsed() < NAMING_DEADLINE,
            "naming inspection exceeded 30-second deadline"
        );
        Ok(())
    }
    fn charge(&mut self, value: &Value) -> Result<()> {
        self.tick()?;
        self.bytes = self
            .bytes
            .checked_add(serde_json::to_vec(value)?.len())
            .ok_or_else(|| anyhow!("naming byte budget overflow"))?;
        ensure!(
            self.bytes <= MAX_NAMING_BYTES,
            "naming inspection exceeded aggregate byte budget"
        );
        Ok(())
    }
    fn value_work(&mut self, value: &Value, depth: usize) -> Result<()> {
        self.tick()?;
        ensure!(
            depth <= crate::tool_schema::MAX_SCHEMA_DEPTH,
            "observed value nesting exceeds depth limit"
        );
        match value {
            Value::Object(map) => {
                for child in map.values() {
                    self.value_work(child, depth + 1)?;
                }
            }
            Value::Array(items) => {
                for child in items {
                    self.value_work(child, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn schema(
        &mut self,
        location: &str,
        schema: &Schema,
        definitions: Option<&str>,
        origin: SchemaEvidenceOrigin,
    ) -> Result<()> {
        ensure!(
            self.roots.len() < MAX_NAMING_ROOTS,
            "naming inspection exceeded aggregate roots"
        );
        self.charge(schema.as_value())?;
        validate_schema_document(schema.as_value())?;
        let context = match definitions {
            Some(path)
                if schema
                    .as_value()
                    .pointer(
                        path.strip_prefix('#')
                            .unwrap_or(path)
                            .strip_suffix('/')
                            .unwrap_or(path.strip_prefix('#').unwrap_or(path)),
                    )
                    .is_some() =>
            {
                NamingSchemaContext::new(schema).with_definitions_path(path)?
            }
            _ => NamingSchemaContext::new(schema),
        };
        // The maintained registry owns URI fragments, anchors and JSON pointers.
        // Structural admission above prohibits external references before preparation.
        let registry = jsonschema::Registry::new()
            .add("urn:veoveo:conformance:schema", schema.as_value())?
            .prepare()?;
        let mut walker = Walker {
            inspection: self,
            context,
            registry: &registry,
            document: schema.as_value(),
            literal_validators: BTreeMap::new(),
            literal_evaluations: BTreeMap::new(),
            done: BTreeSet::new(),
            visiting: BTreeMap::new(),
            source_review: false,
            reached: BTreeSet::new(),
            definitions: Vec::new(),
        };
        walker.node(schema.as_value(), "#", Context::default(), 0, 0)?;
        if let Some(path) = definitions {
            if let Some(container) = schema
                .as_value()
                .pointer(
                    path.strip_prefix('#')
                        .unwrap_or(path)
                        .strip_suffix('/')
                        .unwrap_or(path.strip_prefix('#').unwrap_or(path)),
                )
                .and_then(Value::as_object)
            {
                for (name, definition) in container {
                    walker
                        .definitions
                        .push((definition, format!("{path}{name}")));
                }
            }
        }
        // A reached definition is inspected in every actual use context. Only
        // unused definitions need an additional unclassified inspection.
        let mut index = 0;
        while index < walker.definitions.len() {
            let (definition, path) = walker.definitions[index].clone();
            index += 1;
            if !walker
                .reached
                .contains(&(definition as *const Value as usize))
            {
                walker.node(definition, &path, Context::default(), 0, 0)?;
            }
        }
        let review = walker.source_review;
        self.roots.push(NamingRootInspection {
            location: location.into(),
            origin,
            observations: 0,
            source_review_required: review || origin == SchemaEvidenceOrigin::SourceOnly,
        });
        Ok(())
    }
    pub fn owner(&mut self, evidence: &OwnerSchemaEvidence) -> Result<()> {
        self.schema(
            evidence.label(),
            evidence.schema(),
            Some(evidence.definitions_path()),
            evidence.origin(),
        )?;
        self.tick()?;
        let validator = jsonschema::validator_for(evidence.schema().as_value())?;
        self.tick()?;
        for observation in evidence.observations() {
            self.charge(observation.value())?;
            self.value_work(observation.value(), 0)?;
            ensure!(
                validator.is_valid(observation.value()),
                "owner observation does not satisfy its generated schema"
            );
            self.tick()?;
        }
        self.roots
            .last_mut()
            .expect("root recorded after schema inspection")
            .observations = evidence.observations().len();
        Ok(())
    }
}

struct DictionaryContext {
    key: Value,
    identity: String,
    validator: jsonschema::Validator,
}

#[derive(Clone, Default)]
struct Context {
    exempt: bool,
    scalar: Option<ScalarNaming>,
    field_values: bool,
    dictionary: Option<std::sync::Arc<DictionaryContext>>,
}
impl Context {
    fn key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.dictionary
                .as_ref()
                .map(|key| key.identity.as_str())
                .unwrap_or_default(),
            self.exempt,
            self.field_values,
            self.scalar
                .as_ref()
                .map(|s| serde_json::to_string(s).expect("scalar declaration serializes"))
                .unwrap_or_default()
        )
    }
    fn child(&self) -> Self {
        Self {
            exempt: self.exempt,
            ..Self::default()
        }
    }
}
struct Walker<'a, 'r> {
    inspection: &'a mut NamingInspection,
    context: NamingSchemaContext<'r>,
    registry: &'r jsonschema::Registry<'r>,
    document: &'r Value,
    literal_validators: BTreeMap<usize, std::sync::Arc<jsonschema::Validator>>,
    literal_evaluations: BTreeMap<(usize, usize), BTreeSet<(String, String)>>,
    done: BTreeSet<(usize, String)>,
    visiting: BTreeMap<(usize, String), usize>,
    source_review: bool,
    reached: BTreeSet<usize>,
    definitions: Vec<(&'r Value, String)>,
}
// Validation uses a private copy of the captured key schema. Local references
// are bound to the inspected document's registry; emitted schemas are untouched.
fn bind_key_references(node: &mut Value) -> Result<()> {
    let Some(map) = node.as_object_mut() else {
        return Ok(());
    };
    if let Some(reference) = map.get_mut("$ref") {
        let text = reference
            .as_str()
            .ok_or_else(|| anyhow!("key reference must be text"))?;
        ensure!(text.starts_with('#'), "external key reference is forbidden");
        let base = jsonschema::uri::from_str("urn:veoveo:conformance:schema")?;
        *reference =
            Value::String(jsonschema::uri::resolve_against(&base.borrow(), text)?.to_string());
    }
    for keyword in [
        "properties",
        "patternProperties",
        "dependentSchemas",
        "dependencies",
        "$defs",
        "definitions",
    ] {
        if let Some(Value::Object(children)) = map.get_mut(keyword) {
            for child in children.values_mut() {
                bind_key_references(child)?;
            }
        }
    }
    for keyword in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(Value::Array(children)) = map.get_mut(keyword) {
            for child in children {
                bind_key_references(child)?;
            }
        }
    }
    for keyword in [
        "items",
        "additionalItems",
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
        if let Some(child) = map.get_mut(keyword) {
            if keyword == "items" && child.is_array() {
                for item in child.as_array_mut().expect("array checked") {
                    bind_key_references(item)?;
                }
            } else {
                bind_key_references(child)?;
            }
        }
    }
    Ok(())
}

impl<'a, 'r> Walker<'a, 'r> {
    fn same_instance_profile(
        &mut self,
        node: &Value,
        seen: &mut BTreeSet<usize>,
        depth: usize,
    ) -> Result<Option<veoveo_types::NamingProfile>> {
        self.inspection.tick()?;
        ensure!(
            depth <= crate::tool_schema::MAX_SCHEMA_DEPTH,
            "naming role reference depth exceeded"
        );
        if !seen.insert(node as *const Value as usize) {
            return Ok(None);
        }
        let mut selected =
            veoveo_types::naming_profile(&Schema::try_from(node.clone())?, self.context)?;
        let mut children = vec![];
        if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
            ensure!(
                reference.starts_with('#'),
                "external naming reference is forbidden"
            );
            let resolved = self
                .registry
                .resolver(jsonschema::uri::from_str("urn:veoveo:conformance:schema")?)
                .lookup(reference)?;
            children.push(resolved.contents());
        }
        children.extend(
            node.get("allOf")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        );
        for child in children {
            if let Some(profile) = self.same_instance_profile(child, seen, depth + 1)? {
                ensure!(
                    selected
                        .as_ref()
                        .is_none_or(|previous| previous == &profile),
                    "conflicting same-instance naming roles"
                );
                selected = Some(profile);
            }
        }
        for keyword in ["anyOf", "oneOf"] {
            if let Some(branches) = node.get(keyword).and_then(Value::as_array) {
                let mut profiles = Vec::new();
                let mut complete = true;
                for branch in branches {
                    if branch.get("type").and_then(Value::as_str) == Some("null") {
                        continue;
                    }
                    match self.same_instance_profile(branch, seen, depth + 1)? {
                        Some(profile) => profiles.push(profile),
                        None => complete = false,
                    }
                }
                if complete && let Some(profile) = profiles.first() {
                    ensure!(
                        profiles.iter().all(|other| other == profile),
                        "conflicting alternative naming roles"
                    );
                    ensure!(
                        selected.as_ref().is_none_or(|other| other == profile),
                        "conflicting same-instance alternative role"
                    );
                    selected = Some(profile.clone());
                }
            }
        }
        seen.remove(&(node as *const Value as usize));
        Ok(selected)
    }
    fn dictionary_key_node(
        &mut self,
        node: &'r Value,
        seen: &mut BTreeSet<usize>,
        depth: usize,
    ) -> Result<Option<(&'r Value, &'r Value)>> {
        self.inspection.tick()?;
        ensure!(
            depth <= crate::tool_schema::MAX_SCHEMA_DEPTH,
            "dictionary role reference depth exceeded"
        );
        if !seen.insert(node as *const Value as usize) {
            return Ok(None);
        }
        if let Some(role) = node
            .get(veoveo_types::naming::NAMING_PROFILE_KEY)
            .and_then(|marker| marker.get("role"))
        {
            if role.get("kind").and_then(Value::as_str) == Some("dictionary") {
                return Ok(role.get("keySchema").map(|key| (key, node)));
            }
        }
        if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
            let target = self
                .registry
                .resolver(jsonschema::uri::from_str("urn:veoveo:conformance:schema")?)
                .lookup(reference)?;
            if let Some(key) = self.dictionary_key_node(target.contents(), seen, depth + 1)? {
                return Ok(Some(key));
            }
        }
        for keyword in ["allOf", "anyOf", "oneOf"] {
            for branch in node
                .get(keyword)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(key) = self.dictionary_key_node(branch, seen, depth + 1)? {
                    return Ok(Some(key));
                }
            }
        }
        Ok(None)
    }
    fn node(
        &mut self,
        node: &'r Value,
        path: &str,
        mut context: Context,
        depth: usize,
        progress: usize,
    ) -> Result<()> {
        self.inspection.tick()?;
        ensure!(
            depth <= crate::tool_schema::MAX_SCHEMA_DEPTH,
            "naming schema nesting exceeds depth limit"
        );
        self.reached.insert(node as *const Value as usize);
        let direct = veoveo_types::naming_profile(&Schema::try_from(node.clone())?, self.context)?;
        let profile = self.same_instance_profile(node, &mut BTreeSet::new(), 0)?;
        let mut dictionary = context.dictionary.is_some();
        if let Some(profile) = &profile {
            match profile.role() {
                NamingRole::Scalar { profile } => {
                    ensure!(
                        context
                            .scalar
                            .as_ref()
                            .is_none_or(|existing| existing == profile),
                        "conflicting scalar naming context at {path}"
                    );
                    veoveo_types::with_naming_profile(
                        Schema::try_from(node.clone())?,
                        veoveo_types::NamingProfile::new(NamingRole::Scalar {
                            profile: profile.clone(),
                        })?,
                        self.context,
                    )?;
                    context.scalar = Some(profile.clone());
                }
                NamingRole::Dictionary { key_schema } => {
                    dictionary = true;
                    let (actual_key, source) = self
                        .dictionary_key_node(node, &mut BTreeSet::new(), 0)?
                        .ok_or_else(|| anyhow!("dictionary has no captured key schema"))?;
                    let _ = source;
                    ensure!(
                        actual_key == key_schema.as_value(),
                        "dictionary key source disagrees with admitted profile"
                    );
                    self.node(
                        actual_key,
                        &format!("{path}/keySchema"),
                        Context::default(),
                        depth + 1,
                        progress + 1,
                    )?;
                    if let Some(existing) = &context.dictionary {
                        ensure!(
                            existing.key == *actual_key,
                            "conflicting dictionary key context at {path}"
                        );
                    } else {
                        let mut key = actual_key.clone();
                        bind_key_references(&mut key)?;
                        context.dictionary = Some(std::sync::Arc::new(DictionaryContext {
                            key: actual_key.clone(),
                            identity: serde_json::to_string(actual_key)?,
                            validator: jsonschema::options()
                                .with_draft(
                                    self.document
                                        .get("$schema")
                                        .and_then(Value::as_str)
                                        .map(jsonschema::Draft::from_schema_uri)
                                        .unwrap_or_default(),
                                )
                                .with_registry(self.registry)
                                .build(&key)?,
                        }));
                        self.inspection.tick()?;
                    }
                }
                NamingRole::Jwt { .. }
                | NamingRole::Frozen { .. }
                | NamingRole::External { .. } => {
                    context.exempt |= direct.is_some();
                    self.source_review = true;
                }
            }
        }
        let key_validator = context.dictionary.as_ref().map(|key| &key.validator);
        let key = (node as *const Value as usize, context.key());
        if self.done.contains(&key) {
            return Ok(());
        }
        if let Some(previous) = self.visiting.get(&key) {
            ensure!(
                progress > *previous,
                "nonprogressing reference cycle at {path}"
            );
            return Ok(());
        }
        self.visiting.insert(key.clone(), progress);
        if let Some(properties) = node.get("properties").and_then(Value::as_object) {
            for name in properties.keys() {
                self.inspection.tick()?;
                if let Some(validator) = key_validator {
                    ensure!(
                        validator.is_valid(&Value::String(name.clone())),
                        "dictionary property does not satisfy captured key schema at {path}/{name}"
                    );
                    self.inspection.tick()?;
                }
                ensure!(
                    dictionary || context.exempt || dto_field_name(name),
                    "non-camelCase DTO field at {path}/properties/{name}"
                );
            }
        }
        for keyword in ["required", "dependentRequired", "dependencies"] {
            if context.exempt {
                continue;
            }
            match node.get(keyword) {
                Some(Value::Array(names)) => {
                    for name in names.iter().filter_map(Value::as_str) {
                        self.inspection.tick()?;
                        ensure!(
                            if let Some(validator) = key_validator {
                                validator.is_valid(&Value::String(name.into()))
                            } else {
                                dto_field_name(name)
                            },
                            "invalid required field at {path}/{keyword}"
                        );
                    }
                }
                Some(Value::Object(map)) => {
                    for (name, names) in map {
                        self.inspection.tick()?;
                        ensure!(
                            if let Some(validator) = key_validator {
                                validator.is_valid(&Value::String(name.into()))
                            } else {
                                dto_field_name(name)
                            },
                            "invalid dependent field at {path}/{keyword}"
                        );
                        for name in names
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                        {
                            self.inspection.tick()?;
                            ensure!(
                                if let Some(validator) = key_validator {
                                    validator.is_valid(&Value::String(name.into()))
                                } else {
                                    dto_field_name(name)
                                },
                                "invalid dependent field at {path}/{keyword}"
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        if !context.exempt {
            if let Some(map) = node.get("dependentSchemas").and_then(Value::as_object) {
                for name in map.keys() {
                    self.inspection.tick()?;
                    ensure!(
                        if let Some(validator) = key_validator {
                            validator.is_valid(&Value::String(name.clone()))
                        } else {
                            dto_field_name(name)
                        },
                        "invalid dependent schema field at {path}/{name}"
                    );
                }
            }
        }
        if !context.exempt {
            for value in node
                .get("enum")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .chain(node.get("const"))
            {
                self.inspection.tick()?;
                self.literal(node, value, path, context.clone(), 0, &mut BTreeSet::new())?;
            }
            if let Some(profile) = &context.scalar {
                if !matches!(profile, ScalarNaming::Builtin { .. }) {
                    self.source_review = true;
                }
            }
        }
        if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
            ensure!(
                reference.starts_with('#'),
                "external naming reference is forbidden"
            );
            let resolved = self
                .registry
                .resolver(jsonschema::uri::from_str("urn:veoveo:conformance:schema")?)
                .lookup(reference)?;
            self.node(
                resolved.contents(),
                reference,
                context.clone(),
                depth,
                progress,
            )?;
        }
        for (keyword, child) in schema_children(node, true) {
            let definition = keyword.starts_with("$defs/") || keyword.starts_with("definitions/");
            if definition {
                self.definitions.push((child, format!("{path}/{keyword}")));
                continue;
            }
            let same = [
                "allOf/",
                "anyOf/",
                "oneOf/",
                "dependentSchemas/",
                "dependencies/",
            ]
            .iter()
            .any(|p| keyword.starts_with(p))
                || matches!(keyword.as_str(), "if" | "then" | "else" | "not");
            let next = if definition {
                Context::default()
            } else if same {
                context.clone()
            } else if keyword == "propertyNames" {
                Context {
                    field_values: !dictionary,
                    ..context.child()
                }
            } else {
                context.child()
            };
            self.node(
                child,
                &format!("{path}/{keyword}"),
                next,
                depth + 1,
                progress + usize::from(!same && !definition),
            )?;
        }
        self.visiting.remove(&key);
        self.done.insert(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
