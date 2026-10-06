//! Local schema naming declarations. These declarations confer no authority or attestation.
use crate::{Check, Checked};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

mod dictionary;

pub const NAMING_PROFILE_KEY: &str = "ai.veoveo/naming-profile";

/// Built-in grammars describe existing admission, independently of DTO field casing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Vocabulary)]
pub enum ScalarGrammar {
    ScopeToken,
    ScopeName,
    TaskTypeName,
    ServerSlug,
    ResourceScheme,
    ResourceUri,
    ResourceTemplate,
    ResourceSelectorTemplate,
    ResourcePrefix,
    ExtensionName,
    FormatTag,
}

/// Admitted informational text. Source review establishes its semantic truth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct NamingLabel(Checked<NamingLabelValue>);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
struct NamingLabelValue(String);
impl Check for NamingLabelValue {
    type Error = NamingProfileError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.0.trim().is_empty() || self.0.chars().any(char::is_control) {
            Err(NamingProfileError::Declaration)
        } else {
            Ok(())
        }
    }
}
impl NamingLabel {
    pub fn new(text: impl Into<String>) -> Result<Self, NamingProfileError> {
        Checked::new(NamingLabelValue(text.into())).map(Self)
    }
    pub fn as_str(&self) -> &str {
        &self.0.get().0
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NamingAuthority {
    Owner { module: NamingLabel },
    Standard { document: crate::HttpsUrl },
    Schema { document: crate::ResourceUri },
}
/// These references describe a local schema declaration; they select no executable grammar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamingDeclaration {
    pub authority: NamingAuthority,
    pub profile: NamingLabel,
    pub version: NamingLabel,
    pub applicability: NamingLabel,
}
impl Check for NamingDeclaration {
    type Error = NamingProfileError;
    fn check(&self) -> Result<(), Self::Error> {
        Ok(())
    } // All fields admit through their own immutable decoders.
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ScalarNaming {
    Builtin { grammar: ScalarGrammar },
    OwnerIdentity { declaration: NamingDeclaration },
    OwnerResource { declaration: NamingDeclaration },
    Standard { declaration: NamingDeclaration },
}
impl ScalarNaming {
    pub const fn builtin(grammar: ScalarGrammar) -> Self {
        Self::Builtin { grammar }
    }
    pub fn owner(authority: &str, profile: &str) -> Result<Self, NamingProfileError> {
        Ok(Self::OwnerIdentity {
            declaration: NamingDeclaration {
                authority: NamingAuthority::Owner {
                    module: NamingLabel::new(authority)?,
                },
                profile: NamingLabel::new(profile)?,
                version: NamingLabel::new("unversioned")?,
                applicability: NamingLabel::new("the declared scalar identity schema")?,
            },
        })
    }
    pub fn owner_resource(authority: &str, profile: &str) -> Result<Self, NamingProfileError> {
        let Self::OwnerIdentity { mut declaration } = Self::owner(authority, profile)? else {
            unreachable!()
        };
        declaration.applicability = NamingLabel::new("the declared scalar resource schema")?;
        Ok(Self::OwnerResource { declaration })
    }
    fn check(&self) -> Result<(), NamingProfileError> {
        match self {
            Self::Builtin { .. } => Ok(()),
            Self::OwnerIdentity { declaration } | Self::OwnerResource { declaration } => {
                if !matches!(
                    &declaration.authority,
                    NamingAuthority::Owner { .. } | NamingAuthority::Schema { .. }
                ) {
                    return Err(NamingProfileError::Declaration);
                }
                declaration.check()
            }
            Self::Standard { declaration } => {
                if !matches!(
                    &declaration.authority,
                    NamingAuthority::Standard { .. } | NamingAuthority::Schema { .. }
                ) {
                    return Err(NamingProfileError::Declaration);
                }
                declaration.check()
            }
        }
    }
    pub fn check_spelling(&self, text: &str) -> Result<SpellingCheck, NamingProfileError> {
        let admitted = match self {
            Self::OwnerIdentity { .. } | Self::OwnerResource { .. } | Self::Standard { .. } => {
                return Ok(SpellingCheck::DeclaredSchemaRequired);
            }
            Self::Builtin { grammar } => match grammar {
                ScalarGrammar::ScopeToken => crate::is_scope_token(text),
                ScalarGrammar::ScopeName => crate::ScopeName::parse(text).is_ok(),
                ScalarGrammar::TaskTypeName => crate::TaskTypeName::new(text).is_ok(),
                ScalarGrammar::ServerSlug => crate::ServerSlug::parse(text).is_ok(),
                ScalarGrammar::ResourceScheme => crate::ResourceScheme::parse(text).is_ok(),
                ScalarGrammar::ResourceUri => crate::ResourceUri::new(text).is_ok(),
                ScalarGrammar::ResourceTemplate => crate::ResourceTemplateUri::new(text).is_ok(),
                ScalarGrammar::ResourceSelectorTemplate => {
                    crate::ResourceUriTemplate::new(text).is_ok()
                }
                ScalarGrammar::ResourcePrefix => crate::ResourceUriPrefix::new(text).is_ok(),
                ScalarGrammar::ExtensionName => crate::ExtensionName::parse(text).is_ok(),
                ScalarGrammar::FormatTag => format_tag(text),
            },
        };
        if admitted {
            Ok(SpellingCheck::BuiltinValidated)
        } else {
            Err(NamingProfileError::Grammar)
        }
    }
}
/// Declarations cannot report owner/schema semantics as remotely proved grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellingCheck {
    BuiltinValidated,
    DeclaredSchemaRequired,
}
fn format_tag(text: &str) -> bool {
    let Some(path) = text.strip_prefix("veoveo.ai/") else {
        return false;
    };
    let mut segments: Vec<_> = path.split('/').collect();
    let Some(version) = segments.pop().and_then(|s| s.strip_prefix('v')) else {
        return false;
    };
    !segments.is_empty()
        && !version.starts_with('0')
        && version.parse::<u32>().is_ok_and(|v| v > 0)
        && segments.iter().all(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && !s.starts_with('-')
                && !s.ends_with('-')
        })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NamingRole {
    Scalar {
        profile: ScalarNaming,
    },
    /// Key schema is produced by the SAME live generator as the map's value schema.
    Dictionary {
        #[schemars(with = "serde_json::Value")]
        key_schema: Schema,
    },
    Jwt {
        declaration: NamingDeclaration,
    },
    Frozen {
        declaration: NamingDeclaration,
    },
    External {
        declaration: NamingDeclaration,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NamingProfileValue {
    #[schemars(range(min = 1, max = 1))]
    revision: u8,
    role: NamingRole,
}
impl Check for NamingProfileValue {
    type Error = NamingProfileError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.revision != 1 {
            return Err(NamingProfileError::Revision);
        }
        match &self.role {
            NamingRole::Scalar { profile } => profile.check(),
            NamingRole::Dictionary { key_schema } => {
                if !key_schema.as_value().is_object() {
                    return Err(NamingProfileError::KeySchema);
                }
                Ok(())
            }
            NamingRole::Jwt { declaration }
            | NamingRole::Frozen { declaration }
            | NamingRole::External { declaration } => declaration.check(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NamingProfile(Checked<NamingProfileValue>);
impl NamingProfile {
    pub fn new(role: NamingRole) -> Result<Self, NamingProfileError> {
        Checked::new(NamingProfileValue { revision: 1, role }).map(Self)
    }
    pub fn role(&self) -> &NamingRole {
        &self.0.role
    }
    pub fn revision(&self) -> u8 {
        self.0.revision
    }
}
impl JsonSchema for NamingProfile {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "NamingProfile".into()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::NamingProfile").into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        NamingProfileValue::json_schema(generator)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NamingProfileError {
    #[error("unsupported naming profile revision")]
    Revision,
    #[error("invalid naming profile declaration")]
    Declaration,
    #[error("invalid naming profile metadata")]
    Metadata,
    #[error("conflicting naming profile metadata")]
    Conflict,
    #[error("naming profile applied to the wrong schema node kind")]
    NodeKind,
    #[error("invalid or unresolved dictionary key schema")]
    KeySchema,
    #[error("dictionary properties disagree with the admitted finite key schema")]
    DictionaryKeys,
    #[error("controlled values disagree with their declared scalar grammar")]
    Grammar,
}

/// Borrowed exported schema and its explicitly admitted definitions container.
/// This carries source context without generating types or executing transforms.
#[derive(Clone, Copy)]
pub struct NamingSchemaContext<'a> {
    root: &'a Schema,
    definitions: Option<&'a str>,
}
impl<'a> NamingSchemaContext<'a> {
    /// Standard JSON Schema `$defs` and `definitions` containers are recognized by traversal.
    pub fn new(root: &'a Schema) -> Self {
        Self {
            root,
            definitions: None,
        }
    }
    /// Admit the owner's actual local definitions location, including escaped JSON pointers.
    pub fn with_definitions_path(mut self, path: &'a str) -> Result<Self, NamingProfileError> {
        let pointer = path.strip_prefix('#').unwrap_or(path);
        let pointer = pointer.strip_suffix('/').unwrap_or(pointer);
        if !pointer.starts_with('/') || pointer.is_empty() {
            return Err(NamingProfileError::KeySchema);
        }
        self.definitions = Some(pointer);
        let container = pointer_target(self, pointer)?;
        let object = container.as_object().ok_or(NamingProfileError::KeySchema)?;
        for schema in object.values() {
            Schema::try_from(schema.clone()).map_err(|_| NamingProfileError::KeySchema)?;
        }
        Ok(self)
    }
    fn as_value(self) -> &'a Value {
        self.root.as_value()
    }
}
/// Parse and check local metadata against its schema node and complete exported schema root.
/// No remote schema or declared normative reference is fetched.
pub fn naming_profile(
    schema: &Schema,
    root: NamingSchemaContext<'_>,
) -> Result<Option<NamingProfile>, NamingProfileError> {
    let Some(value) = schema.get(NAMING_PROFILE_KEY) else {
        return Ok(None);
    };
    let profile: NamingProfile =
        serde_json::from_value(value.clone()).map_err(|_| NamingProfileError::Metadata)?;
    validate_node(schema, root, &profile)?;
    Ok(Some(profile))
}
/// Add a role without changing existing constraints or replacing another role.
pub fn with_naming_profile(
    mut schema: Schema,
    profile: NamingProfile,
    root: NamingSchemaContext<'_>,
) -> Result<Schema, NamingProfileError> {
    if let Some(existing) = naming_profile(&schema, root)? {
        if existing != profile {
            return Err(NamingProfileError::Conflict);
        }
        return Ok(schema);
    }
    validate_node(&schema, root, &profile)?;
    schema.ensure_object().insert(
        NAMING_PROFILE_KEY.into(),
        serde_json::to_value(profile).map_err(|_| NamingProfileError::Metadata)?,
    );
    Ok(schema)
}
fn validate_node(
    schema: &Schema,
    root: NamingSchemaContext<'_>,
    profile: &NamingProfile,
) -> Result<(), NamingProfileError> {
    let node = schema.as_value();
    match profile.role() {
        NamingRole::Scalar { .. } => {
            let shape = instance_shape(node, root, Some(profile), &mut BTreeSet::new(), 0)?;
            if shape & STRING == 0 || shape & OTHER != 0 {
                return Err(NamingProfileError::NodeKind);
            }
        }
        NamingRole::Dictionary { key_schema } => {
            dictionary::validate(node, root, profile, key_schema.as_value())?;
        }
        NamingRole::Jwt { .. } | NamingRole::Frozen { .. } | NamingRole::External { .. } => {
            let shape = instance_shape(node, root, Some(profile), &mut BTreeSet::new(), 0)?;
            if shape & OBJECT == 0 || shape & !(OBJECT | NULL) != 0 {
                return Err(NamingProfileError::NodeKind);
            }
        }
    }
    Ok(())
}
/// Convenience: explicitly request the key type from this generator, then decorate.
/// Inline owner callbacks may run again; callback-sensitive owners use the captured-key seam.
pub fn dictionary_schema<K: JsonSchema>(
    generator: &mut SchemaGenerator,
    schema: Schema,
) -> Result<Schema, NamingProfileError> {
    let key_schema = generator.subschema_for::<K>();
    dictionary_schema_with_key(generator, schema, key_schema)
}
/// Pure decoration using an already-produced key and the existing live definitions.
/// No type callback, transform, key encoder or mapped-value rewrite runs here.
pub fn dictionary_schema_with_key(
    generator: &SchemaGenerator,
    schema: Schema,
    key_schema: Schema,
) -> Result<Schema, NamingProfileError> {
    let root_schema = live_root(generator);
    let root = NamingSchemaContext::new(&root_schema)
        .with_definitions_path(&generator.settings().definitions_path)?;
    with_naming_profile(
        schema,
        NamingProfile::new(NamingRole::Dictionary { key_schema })?,
        root,
    )
}
/// Public owner helper for scalar callbacks. Existing annotations are checked and preserved.
pub fn scalar_schema(schema: Schema, profile: ScalarNaming) -> Result<Schema, NamingProfileError> {
    let root_schema = schema.clone();
    let root = NamingSchemaContext::new(&root_schema);
    if let Some(existing) = naming_profile(&schema, root)? {
        if existing.role()
            != &(NamingRole::Scalar {
                profile: profile.clone(),
            })
        {
            return Err(NamingProfileError::Conflict);
        }
        return Ok(schema);
    }
    with_naming_profile(
        schema,
        NamingProfile::new(NamingRole::Scalar { profile })?,
        root,
    )
}
const STRING: u8 = 1;
const NULL: u8 = 2;
const OBJECT: u8 = 4;
const ARRAY: u8 = 8;
const NUMBER: u8 = 16;
const BOOLEAN: u8 = 32;
const OTHER: u8 = OBJECT | ARRAY | NUMBER | BOOLEAN;
const ANY: u8 = STRING | NULL | OTHER;
fn instance_kind(value: &Value) -> u8 {
    match value {
        Value::String(_) => STRING,
        Value::Null => NULL,
        Value::Object(_) => OBJECT,
        Value::Array(_) => ARRAY,
        Value::Number(_) => NUMBER,
        Value::Bool(_) => BOOLEAN,
    }
}
/// Inspect supported same-instance scalar forms. Constraint bytes are never rewritten.
/// This is not C33's future complete graph/predicate traversal.
fn reject_unsupported_refs(
    node: &Value,
    root: NamingSchemaContext<'_>,
) -> Result<(), NamingProfileError> {
    if [
        "$dynamicRef",
        "$recursiveRef",
        "$dynamicAnchor",
        "$recursiveAnchor",
        "$anchor",
    ]
    .iter()
    .any(|keyword| node.get(keyword).is_some())
        || node != root.as_value() && (node.get("$id").is_some() || node.get("id").is_some())
    {
        Err(NamingProfileError::NodeKind)
    } else {
        Ok(())
    }
}
/// Export-root identity is supported; nested resource identities require a resolver we do not expose.
fn admitted_pointer(path: &str) -> Result<(), NamingProfileError> {
    if !path.starts_with('/') || path.split('/').skip(1).any(str::is_empty) {
        return Err(NamingProfileError::NodeKind);
    }
    let mut bytes = path.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
            return Err(NamingProfileError::NodeKind);
        }
    }
    Ok(())
}
fn local_target<'a>(
    root: NamingSchemaContext<'a>,
    reference: &str,
) -> Result<&'a Value, NamingProfileError> {
    let uri = iri_string::types::UriReferenceStr::new(reference)
        .map_err(|_| NamingProfileError::NodeKind)?;
    if !reference.starts_with('#') {
        return Err(NamingProfileError::NodeKind);
    }
    let fragment = uri.fragment_str().ok_or(NamingProfileError::NodeKind)?;
    let pointer = percent_encoding::percent_decode_str(fragment)
        .decode_utf8()
        .map_err(|_| NamingProfileError::NodeKind)?;
    pointer_target(root, &pointer)
}
fn pointer_target<'a>(
    root: NamingSchemaContext<'a>,
    path: &str,
) -> Result<&'a Value, NamingProfileError> {
    admitted_pointer(path)?;
    // Dictionary entries are names, not schema keywords. Only schema nodes can rebase refs.
    enum PointerRole {
        Schema,
        Names,
        Array,
        ContainerPrefix,
        Unknown,
    }
    let mut role = PointerRole::Schema;
    let mut current = root.as_value();
    let mut start = 1;
    for end in path
        .char_indices()
        .filter_map(|(i, c)| (c == '/' && i > 0).then_some(i))
        .chain(std::iter::once(path.len()))
    {
        let token = &path[start..end];
        role = match role {
            PointerRole::Names | PointerRole::Array => PointerRole::Schema,
            PointerRole::Schema => match token {
                "$defs" | "definitions" | "properties" | "patternProperties"
                | "dependentSchemas" => PointerRole::Names,
                "allOf" | "anyOf" | "oneOf" | "prefixItems" => PointerRole::Array,
                "additionalProperties"
                | "propertyNames"
                | "items"
                | "contains"
                | "not"
                | "if"
                | "then"
                | "else"
                | "unevaluatedProperties"
                | "unevaluatedItems" => PointerRole::Schema,
                _ => PointerRole::Unknown,
            },
            PointerRole::ContainerPrefix | PointerRole::Unknown => PointerRole::Unknown,
        };
        current = root
            .as_value()
            .pointer(&path[..end])
            .ok_or(NamingProfileError::NodeKind)?;
        if root.definitions == Some(&path[..end]) {
            role = PointerRole::Names;
        }
        // Only an exact declared path supplies otherwise-unknown container prefixes.
        // Known schema-bearing paths (such as properties/Widget) keep their schema role.
        if matches!(role, PointerRole::Unknown)
            && root.definitions.is_some_and(|definitions| {
                definitions
                    .strip_prefix(&path[..end])
                    .is_some_and(|tail| tail.starts_with('/'))
            })
        {
            role = PointerRole::ContainerPrefix;
        }
        if matches!(role, PointerRole::Schema | PointerRole::Unknown)
            && (current.get("$id").is_some() || current.get("id").is_some())
        {
            return Err(NamingProfileError::NodeKind);
        }
        start = end + 1;
    }
    Ok(current)
}
fn live_root(generator: &SchemaGenerator) -> Schema {
    let path = &generator.settings().definitions_path;
    let path = path.strip_prefix('#').unwrap_or(path);
    let path = path.strip_suffix('/').unwrap_or(path);
    let mut root = serde_json::json!({});
    if path.is_empty() {
        return Schema::try_from(Value::Object(generator.definitions().clone()))
            .expect("local definitions context");
    }
    let mut current = &mut root;
    for part in path
        .strip_prefix('/')
        .expect("generator definitions path must be a local JSON pointer")
        .split('/')
    {
        let key = part.replace("~1", "/").replace("~0", "~");
        current = current
            .as_object_mut()
            .expect("definitions path object")
            .entry(key)
            .or_insert_with(|| serde_json::json!({}));
    }
    *current = Value::Object(generator.definitions().clone());
    Schema::try_from(root).expect("local definitions context")
}
fn instance_shape(
    node: &Value,
    root: NamingSchemaContext<'_>,
    selected: Option<&NamingProfile>,
    refs: &mut BTreeSet<String>,
    depth: usize,
) -> Result<u8, NamingProfileError> {
    reject_unsupported_refs(node, root)?;
    if depth >= 64 {
        return Err(NamingProfileError::NodeKind);
    }
    if node == &Value::Bool(false) {
        return Ok(0);
    }
    let object = node.as_object().ok_or(NamingProfileError::NodeKind)?;
    if let Some(marker) = object.get(NAMING_PROFILE_KEY) {
        let marker: NamingProfile =
            serde_json::from_value(marker.clone()).map_err(|_| NamingProfileError::Metadata)?;
        if selected.is_some_and(|s| s != &marker) {
            return Err(NamingProfileError::Conflict);
        }
    }
    let mut shape = ANY;
    if let Some(types) = object.get("type") {
        let types: Vec<&str> = match types {
            Value::String(t) => vec![t],
            Value::Array(ts) if !ts.is_empty() => ts
                .iter()
                .map(|v| v.as_str().ok_or(NamingProfileError::NodeKind))
                .collect::<Result<_, _>>()?,
            _ => return Err(NamingProfileError::NodeKind),
        };
        let mut allowed = 0;
        for t in types {
            allowed |= match t {
                "string" => STRING,
                "null" => NULL,
                "object" => OBJECT,
                "array" => ARRAY,
                "integer" | "number" => NUMBER,
                "boolean" => BOOLEAN,
                _ => return Err(NamingProfileError::NodeKind),
            };
        }
        shape &= allowed;
    }
    if let Some(reference) = object.get("$ref") {
        let reference = reference.as_str().ok_or(NamingProfileError::NodeKind)?;
        if !reference.starts_with("#/") || !refs.insert(reference.into()) {
            return Err(NamingProfileError::NodeKind);
        }
        let target = local_target(root, reference)?;
        shape &= instance_shape(target, root, selected, refs, depth + 1)?;
        refs.remove(reference);
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = object.get(keyword) {
            let branches = branches
                .as_array()
                .filter(|b| !b.is_empty())
                .ok_or(NamingProfileError::NodeKind)?;
            let mut result = if keyword == "allOf" { ANY } else { 0 };
            for branch in branches {
                let branch = instance_shape(branch, root, selected, refs, depth + 1)?;
                if keyword == "allOf" {
                    result &= branch;
                } else {
                    result |= branch;
                }
            }
            shape &= result;
        }
    }
    for keyword in ["enum", "const"] {
        if let Some(values) = object.get(keyword) {
            let values: Vec<&Value> = if keyword == "enum" {
                values
                    .as_array()
                    .filter(|v| !v.is_empty())
                    .ok_or(NamingProfileError::NodeKind)?
                    .iter()
                    .collect()
            } else {
                vec![values]
            };
            let mut allowed = 0;
            for value in values {
                allowed |= instance_kind(value);
                if let (Some(NamingRole::Scalar { profile }), Some(text)) =
                    (selected.map(NamingProfile::role), value.as_str())
                {
                    profile.check_spelling(text)?;
                }
            }
            shape &= allowed;
        }
    }
    Ok(shape)
}
fn inherited_profile(
    node: &Value,
    root: NamingSchemaContext<'_>,
    refs: &mut BTreeSet<String>,
    depth: usize,
) -> Result<Option<NamingProfile>, NamingProfileError> {
    reject_unsupported_refs(node, root)?;
    if depth >= 64 {
        return Err(NamingProfileError::NodeKind);
    }
    let mut found = None;
    if let Some(value) = node.get(NAMING_PROFILE_KEY) {
        found =
            Some(serde_json::from_value(value.clone()).map_err(|_| NamingProfileError::Metadata)?);
    }
    let mut children = Vec::new();
    if let Some(reference) = node.get("$ref") {
        let reference = reference.as_str().ok_or(NamingProfileError::NodeKind)?;
        if !reference.starts_with("#/") || !refs.insert(reference.into()) {
            return Err(NamingProfileError::NodeKind);
        }
        let child = local_target(root, reference)?;
        children.push(inherited_profile(child, root, refs, depth + 1)?);
        refs.remove(reference);
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = node.get(keyword) {
            for child in branches.as_array().ok_or(NamingProfileError::NodeKind)? {
                children.push(inherited_profile(child, root, refs, depth + 1)?);
            }
        }
    }
    for other in children.into_iter().flatten() {
        if found.as_ref().is_some_and(|p| p != &other) {
            return Err(NamingProfileError::Conflict);
        }
        found = Some(other);
    }
    Ok(found)
}
/// Explicitly static generated declaration path; dynamic callers use the fallible public helpers.
pub fn static_identity_schema(
    schema: Schema,
    selected: Option<ScalarNaming>,
    owner: &'static str,
    name: &'static str,
    generator: &mut SchemaGenerator,
) -> Schema {
    emit_declaration_schema(
        schema,
        selected,
        ScalarNaming::owner(owner, name).expect("static identity declaration"),
        generator,
    )
}
pub fn static_resource_schema(
    schema: Schema,
    selected: Option<ScalarNaming>,
    owner: &'static str,
    name: &'static str,
    generator: &mut SchemaGenerator,
) -> Schema {
    emit_declaration_schema(
        schema,
        selected,
        ScalarNaming::owner_resource(owner, name).expect("static resource declaration"),
        generator,
    )
}
fn emit_declaration_schema(
    schema: Schema,
    selected: Option<ScalarNaming>,
    default: ScalarNaming,
    generator: &mut SchemaGenerator,
) -> Schema {
    let root_schema = live_root(generator);
    let root = NamingSchemaContext::new(&root_schema)
        .with_definitions_path(&generator.settings().definitions_path)
        .expect("live generator definitions container");
    if let Some(existing) =
        naming_profile(&schema, root).expect("owner schema naming metadata must be admitted")
    {
        if let Some(selected) = selected {
            assert_eq!(
                existing.role(),
                &NamingRole::Scalar { profile: selected },
                "conflicting owner naming selection"
            );
        }
        return schema;
    }
    if let Some(inherited) = inherited_profile(schema.as_value(), root, &mut BTreeSet::new(), 0)
        .expect("callback classifications must agree")
    {
        if let Some(selected) = selected {
            assert_eq!(
                inherited.role(),
                &NamingRole::Scalar { profile: selected },
                "conflicting inherited naming selection"
            );
        }
        validate_node(&schema, root, &inherited)
            .expect("callback use site must preserve its referenced classification");
        return schema; // The real referenced definition already owns the annotation; no use-site alias is invented.
    }
    let shape = instance_shape(schema.as_value(), root, None, &mut BTreeSet::new(), 0)
        .expect("generated identity/address schema must describe an admitted node kind");
    assert_ne!(
        shape, ANY,
        "identity/address callback must constrain the instance form"
    );
    if shape & OTHER != 0 || shape == 0 {
        assert!(
            selected.is_none(),
            "scalar profile cannot annotate a structured identity/address schema"
        );
        return schema;
    }
    with_naming_profile(
        schema,
        NamingProfile::new(NamingRole::Scalar {
            profile: selected.unwrap_or(default),
        })
        .expect("static declaration"),
        root,
    )
    .expect("owner schema must match naming declaration")
}
