//! Installation-bound target admission. Owner codecs never activate owner runtimes.
use crate::{AuditTarget, model::CoreAuditTarget};
use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    any::{Any, TypeId},
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
    sync::Arc,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuditTargetError {
    #[error(
        "audit target codec `{0}` is not configured; register the supported owner's read codec"
    )]
    Unbound(String),
    #[error("duplicate or core-colliding audit target codec `{0}`")]
    Collision(String),
    #[error("audit target belongs to another registry")]
    Registry,
    #[error("audit target owner type does not match registration")]
    Type,
    #[error("invalid closed audit target payload or lookup reference")]
    Invalid,
    #[error("audit target schema definition collision `{0}`")]
    SchemaCollision(String),
}

/// Owner-controlled projection, admitted before any storage driver receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditLookupReference {
    table: String,
    key: AuditLookupKey,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditLookupKey {
    Uuid(uuid::Uuid),
    Text(String),
}
impl AuditLookupReference {
    pub fn new(table: impl Into<String>, key: AuditLookupKey) -> Result<Self, AuditTargetError> {
        let table = table.into();
        if table.is_empty()
            || !table
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !table.as_bytes()[0].is_ascii_alphabetic()
            || matches!(&key, AuditLookupKey::Text(s) if s.is_empty())
        {
            return Err(AuditTargetError::Invalid);
        }
        Ok(Self { table, key })
    }
    pub fn table(&self) -> &str {
        &self.table
    }
    pub fn key(&self) -> &AuditLookupKey {
        &self.key
    }
}

/// A closed owner object includes its discriminator in its existing wire shape.
pub trait AuditTargetOwner:
    Clone + std::fmt::Debug + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static
{
    const KIND: &'static str;
    fn lookup_reference(&self) -> Result<Option<AuditLookupReference>, AuditTargetError>;
}
#[derive(Clone)]
pub struct AuditTargetRegistration<T> {
    identity: Arc<()>,
    marker: PhantomData<fn() -> T>,
}
impl<T: AuditTargetOwner> AuditTargetRegistration<T> {
    pub fn target(
        &self,
        registry: &AuditTargetRegistry,
        value: T,
    ) -> Result<AuditTarget, AuditTargetError> {
        if !Arc::ptr_eq(&self.identity, &registry.identity) {
            return Err(AuditTargetError::Registry);
        }
        registry.target(value)
    }
    pub fn get<'a>(
        &self,
        registry: &AuditTargetRegistry,
        target: &'a AuditTarget,
    ) -> Result<&'a T, AuditTargetError> {
        if !Arc::ptr_eq(&self.identity, &registry.identity) {
            return Err(AuditTargetError::Registry);
        }
        registry.validate(target)?;
        match target {
            AuditTarget::Extension(value) => {
                value.payload.downcast_ref().ok_or(AuditTargetError::Type)
            }
            _ => Err(AuditTargetError::Type),
        }
    }
}
type Payload = Arc<dyn Any + Send + Sync>;
type Decode = fn(Value) -> Result<(Value, Payload, Option<AuditLookupReference>), AuditTargetError>;
struct Codec {
    type_id: TypeId,
    decode: Decode,
    schema: Value,
}
#[derive(Clone)]
pub struct AuditTargetRegistry {
    identity: Arc<()>,
    codecs: Arc<BTreeMap<String, Codec>>,
}
impl std::fmt::Debug for AuditTargetRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditTargetRegistry")
            .field("kinds", &self.codecs.keys())
            .finish()
    }
}
pub struct AuditTargetRegistryBuilder {
    identity: Arc<()>,
    codecs: BTreeMap<String, Codec>,
}
const CORE: &[&str] = &[
    "platform_resource",
    "server",
    "artifact",
    "task",
    "task_route",
    "principal",
    "work_context",
    "tool",
    "resource",
    "resource_template",
    "prompt",
    "discovery",
    "profile",
    "client",
    "audit_log",
    "installation",
];
impl AuditTargetRegistryBuilder {
    pub fn register<T: AuditTargetOwner>(
        &mut self,
    ) -> Result<AuditTargetRegistration<T>, AuditTargetError> {
        if T::KIND.is_empty()
            || CORE.contains(&T::KIND)
            || self.codecs.contains_key(T::KIND)
            || self.codecs.values().any(|c| c.type_id == TypeId::of::<T>())
        {
            return Err(AuditTargetError::Collision(T::KIND.into()));
        }
        let schema = serde_json::to_value(schemars::schema_for!(T))
            .map_err(|_| AuditTargetError::Invalid)?;
        // Every owner branch must declare a closed object and bind this discriminator.
        if !schema
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|fields| fields.iter().any(|field| field.as_str() == Some("kind")))
            || schema.get("additionalProperties") != Some(&Value::Bool(false))
            || schema
                .pointer("/properties/kind/const")
                .and_then(Value::as_str)
                != Some(T::KIND)
        {
            return Err(AuditTargetError::Invalid);
        }
        check_closed_schema(&schema, &schema)?;
        let core_schema = serde_json::to_value(crate::reader::base_reader_schema())
            .map_err(|_| AuditTargetError::Invalid)?;
        if let (Some(owner), Some(core)) = (
            schema.get("$defs").and_then(Value::as_object),
            core_schema.get("$defs").and_then(Value::as_object),
        ) {
            for name in owner.keys() {
                if core.contains_key(name) {
                    return Err(AuditTargetError::SchemaCollision(name.clone()));
                }
            }
        }
        let existing = self
            .codecs
            .values()
            .filter_map(|c| c.schema.get("$defs").and_then(Value::as_object));
        if let Some(defs) = schema.get("$defs").and_then(Value::as_object) {
            for other in existing {
                for name in defs.keys() {
                    if other.contains_key(name) {
                        return Err(AuditTargetError::SchemaCollision(name.clone()));
                    }
                }
            }
        }
        self.codecs.insert(
            T::KIND.into(),
            Codec {
                type_id: TypeId::of::<T>(),
                decode: decode_owner::<T>,
                schema,
            },
        );
        Ok(AuditTargetRegistration {
            identity: self.identity.clone(),
            marker: PhantomData,
        })
    }
    pub fn build(self) -> AuditTargetRegistry {
        AuditTargetRegistry {
            identity: self.identity,
            codecs: Arc::new(self.codecs),
        }
    }
}
fn decode_owner<T: AuditTargetOwner>(
    wire: Value,
) -> Result<(Value, Payload, Option<AuditLookupReference>), AuditTargetError> {
    let payload: T = serde_json::from_value(wire.clone()).map_err(|_| AuditTargetError::Invalid)?;
    let admitted = serde_json::to_value(&payload).map_err(|_| AuditTargetError::Invalid)?;
    if admitted != wire || admitted.get("kind").and_then(Value::as_str) != Some(T::KIND) {
        return Err(AuditTargetError::Invalid);
    }
    let reference = payload.lookup_reference()?;
    Ok((admitted, Arc::new(payload), reference))
}
impl AuditTargetRegistry {
    pub fn builder() -> AuditTargetRegistryBuilder {
        AuditTargetRegistryBuilder {
            identity: Arc::new(()),
            codecs: BTreeMap::new(),
        }
    }
    pub fn empty() -> Self {
        Self::builder().build()
    }
    pub fn registration<T: AuditTargetOwner>(
        &self,
    ) -> Result<AuditTargetRegistration<T>, AuditTargetError> {
        let codec = self
            .codecs
            .get(T::KIND)
            .ok_or_else(|| AuditTargetError::Unbound(T::KIND.into()))?;
        if codec.type_id != TypeId::of::<T>() {
            return Err(AuditTargetError::Type);
        }
        Ok(AuditTargetRegistration {
            identity: self.identity.clone(),
            marker: PhantomData,
        })
    }
    pub fn target<T: AuditTargetOwner>(&self, payload: T) -> Result<AuditTarget, AuditTargetError> {
        let codec = self
            .codecs
            .get(T::KIND)
            .ok_or_else(|| AuditTargetError::Unbound(T::KIND.into()))?;
        if codec.type_id != TypeId::of::<T>() {
            return Err(AuditTargetError::Type);
        }
        self.admit(serde_json::to_value(payload).map_err(|_| AuditTargetError::Invalid)?)
    }
    pub fn validate(&self, target: &AuditTarget) -> Result<(), AuditTargetError> {
        if let AuditTarget::Extension(value) = target
            && !Arc::ptr_eq(&self.identity, &value.identity)
        {
            return Err(AuditTargetError::Registry);
        }
        Ok(())
    }
    pub(crate) fn admit(&self, wire: Value) -> Result<AuditTarget, AuditTargetError> {
        let kind = wire
            .get("kind")
            .and_then(Value::as_str)
            .ok_or(AuditTargetError::Invalid)?;
        if CORE.contains(&kind) {
            if kind == "installation" && wire.as_object().is_none_or(|object| object.len() != 1) {
                return Err(AuditTargetError::Invalid);
            }
            return serde_json::from_value::<CoreAuditTarget>(wire)
                .map(Into::into)
                .map_err(|_| AuditTargetError::Invalid);
        }
        let codec = self
            .codecs
            .get(kind)
            .ok_or_else(|| AuditTargetError::Unbound(kind.into()))?;
        let (wire, payload, reference) = (codec.decode)(wire)?;
        Ok(AuditTarget::Extension(AdmittedAuditTarget {
            identity: self.identity.clone(),
            wire,
            payload,
            reference,
        }))
    }
    pub(crate) fn compose_schema(
        &self,
        mut root: schemars::Schema,
    ) -> Result<schemars::Schema, AuditTargetError> {
        let object = root.as_object_mut().ok_or(AuditTargetError::Invalid)?;
        let defs = object
            .get_mut("$defs")
            .and_then(Value::as_object_mut)
            .ok_or(AuditTargetError::Invalid)?;
        let mut branches = Vec::new();
        for codec in self.codecs.values() {
            let mut schema = codec.schema.clone();
            let schema_object = schema.as_object_mut().ok_or(AuditTargetError::Invalid)?;
            schema_object.remove("$schema");
            if let Some(Value::Object(owner_defs)) = schema_object.remove("$defs") {
                for (name, definition) in owner_defs {
                    if defs.insert(name.clone(), definition).is_some() {
                        return Err(AuditTargetError::SchemaCollision(name));
                    }
                }
            }
            branches.push(schema);
        }
        if let Some(target) = defs.get_mut("AuditTarget") {
            target
                .get_mut("oneOf")
                .and_then(Value::as_array_mut)
                .ok_or(AuditTargetError::Invalid)?
                .extend(branches);
        } else {
            return Err(AuditTargetError::Invalid);
        }
        Ok(root)
    }
}
/// Construction is private; wire equality deliberately excludes registry and reference metadata.
#[derive(Clone)]
pub struct AdmittedAuditTarget {
    identity: Arc<()>,
    wire: Value,
    payload: Payload,
    reference: Option<AuditLookupReference>,
}
impl AdmittedAuditTarget {
    pub fn lookup_reference(&self) -> Option<&AuditLookupReference> {
        self.reference.as_ref()
    }
}
impl PartialEq for AdmittedAuditTarget {
    fn eq(&self, other: &Self) -> bool {
        self.wire == other.wire
    }
}
impl Eq for AdmittedAuditTarget {}
impl std::fmt::Debug for AdmittedAuditTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.wire.fmt(f)
    }
}
impl Serialize for AdmittedAuditTarget {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.wire.serialize(s)
    }
}

/// The admitted owner profile is generated closed objects, scalar constraints,
/// arrays and local definitions. Unrecognized schema mechanics fail registration.
fn check_closed_schema(schema: &Value, root: &Value) -> Result<(), AuditTargetError> {
    check_schema_node(schema, root, &mut BTreeSet::new(), &mut BTreeSet::new())
}

fn check_schema_node(
    schema: &Value,
    root: &Value,
    active: &mut BTreeSet<String>,
    checked: &mut BTreeSet<String>,
) -> Result<(), AuditTargetError> {
    let Value::Object(object) = schema else {
        return if schema == &Value::Bool(false) {
            Ok(())
        } else {
            Err(AuditTargetError::Invalid)
        };
    };
    const KEYWORDS: &[&str] = &[
        "$schema",
        "$defs",
        "$ref",
        "title",
        "description",
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "prefixItems",
        "minItems",
        "maxItems",
        "uniqueItems",
        "oneOf",
        "anyOf",
        "allOf",
        "enum",
        "const",
        "default",
        "format",
        "pattern",
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
        "deprecated",
        "readOnly",
        "writeOnly",
    ];
    if object.keys().any(|key| !KEYWORDS.contains(&key.as_str()))
        || !["type", "$ref", "oneOf", "anyOf", "allOf", "const", "enum"]
            .iter()
            .any(|key| object.contains_key(*key))
    {
        return Err(AuditTargetError::Invalid);
    }
    if let Some(reference) = object.get("$ref") {
        let reference = reference.as_str().ok_or(AuditTargetError::Invalid)?;
        let name = reference
            .strip_prefix("#/$defs/")
            .ok_or(AuditTargetError::Invalid)?;
        if name.is_empty() || name.contains('/') || name.contains('%') {
            return Err(AuditTargetError::Invalid);
        }
        let mut characters = name.chars();
        while let Some(character) = characters.next() {
            if character == '~' && !matches!(characters.next(), Some('0' | '1')) {
                return Err(AuditTargetError::Invalid);
            }
        }
        let definition = root
            .pointer(&reference[1..])
            .ok_or(AuditTargetError::Invalid)?;
        if !checked.contains(name) {
            if !active.insert(name.to_owned()) {
                return Err(AuditTargetError::Invalid);
            }
            check_schema_node(definition, root, active, checked)?;
            active.remove(name);
            checked.insert(name.to_owned());
        }
    }
    if let Some(kind) = object.get("type") {
        let kinds: Vec<&str> = match kind {
            Value::String(kind) => vec![kind.as_str()],
            Value::Array(kinds) => kinds
                .iter()
                .map(|kind| kind.as_str().ok_or(AuditTargetError::Invalid))
                .collect::<Result<_, _>>()?,
            _ => return Err(AuditTargetError::Invalid),
        };
        if kinds.is_empty()
            || kinds.iter().any(|kind| {
                ![
                    "object", "array", "string", "integer", "number", "boolean", "null",
                ]
                .contains(kind)
            })
        {
            return Err(AuditTargetError::Invalid);
        }
        if kinds.contains(&"object")
            && object.get("additionalProperties") != Some(&Value::Bool(false))
        {
            return Err(AuditTargetError::Invalid);
        }
        if kinds.contains(&"array")
            && !object.contains_key("items")
            && !object.contains_key("prefixItems")
        {
            return Err(AuditTargetError::Invalid);
        }
    }
    if let Some(prefix) = object.get("prefixItems") {
        let prefix = prefix.as_array().ok_or(AuditTargetError::Invalid)?;
        if !object.contains_key("items")
            && !object
                .get("maxItems")
                .and_then(Value::as_u64)
                .is_some_and(|maximum| maximum <= prefix.len() as u64)
        {
            return Err(AuditTargetError::Invalid);
        }
    }
    for key in ["properties", "$defs"] {
        if let Some(children) = object.get(key) {
            let children = children.as_object().ok_or(AuditTargetError::Invalid)?;
            for child in children.values() {
                check_schema_node(child, root, active, checked)?;
            }
        }
    }
    for key in ["oneOf", "anyOf", "allOf", "prefixItems"] {
        if let Some(children) = object.get(key) {
            let children = children.as_array().ok_or(AuditTargetError::Invalid)?;
            if children.is_empty() {
                return Err(AuditTargetError::Invalid);
            }
            for child in children {
                check_schema_node(child, root, active, checked)?;
            }
        }
    }
    for key in ["items", "additionalProperties"] {
        if let Some(child) = object.get(key) {
            check_schema_node(child, root, active, checked)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    #[test]
    fn reserved_discriminators_match_every_core_schema_branch() {
        let schema = serde_json::to_value(schemars::schema_for!(CoreAuditTarget)).unwrap();
        let branches = schema
            .get("oneOf")
            .and_then(Value::as_array)
            .expect("closed core union");
        let generated = branches
            .iter()
            .map(|branch| {
                branch
                    .pointer("/properties/kind/const")
                    .and_then(Value::as_str)
                    .expect("core discriminator")
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(generated, CORE.iter().copied().collect());
    }
    #[test]
    fn rejects_schema_holes_in_supported_profile() {
        for schema in [
            serde_json::json!({}),
            serde_json::json!({"type":"array","prefixItems":[{"type":"string"}]}),
            serde_json::json!({"type":"array","prefixItems":[{"type":"string"}],"maxItems":2}),
            serde_json::json!({"type":"array","prefixItems":[{"type":"string"}],"items":true}),
            serde_json::json!({"$ref":"https://example.test/target.json"}),
            serde_json::json!({"$ref":"#/$defs/Missing"}),
            serde_json::json!({"$ref":"#/$defs/A/default","$defs":{"A":{"type":"string","default":{}}}}),
            serde_json::json!({"$ref":"#/$defs/A","$defs":{"A":{"$ref":"#/$defs/B"},"B":{"$ref":"#/$defs/A"}}}),
            serde_json::json!({"$ref":"#/$defs/A","$defs":{"A":{"type":"object","additionalProperties":false,"properties":{"child":{"$ref":"#/$defs/A"}}}}}),
            serde_json::json!({"$ref":"#/$defs/A~2B","$defs":{"A~2B":{"type":"string"}}}),
            serde_json::json!({"type":["object","null"],"properties":{"id":{"type":"string"}}}),
            serde_json::json!({"type":"object","additionalProperties":false,"properties":{"payload":{}}}),
            serde_json::json!({"type":"object","additionalProperties":false,"properties":{"payload":{"type":"object","additionalProperties":{"type":"string"}}}}),
        ] {
            assert!(check_closed_schema(&schema, &schema).is_err(), "{schema}");
        }
        for schema in [
            serde_json::json!({"type":"array","prefixItems":[{"type":"string"}],"items":false}),
            serde_json::json!({"type":"array","prefixItems":[{"type":"string"}],"maxItems":1}),
            serde_json::json!({"type":"array","prefixItems":[{"type":"string"}],"items":{"type":"integer"}}),
        ] {
            assert!(check_closed_schema(&schema, &schema).is_ok(), "{schema}");
        }
        let escaped =
            serde_json::json!({"$ref":"#/$defs/A~1B~0C","$defs":{"A/B~C":{"type":"string"}}});
        assert!(check_closed_schema(&escaped, &escaped).is_ok());
        let closed = serde_json::json!({"type":["object","null"],"additionalProperties":false,"properties":{"id":{"$ref":"#/$defs/Id"}},"$defs":{"Id":{"type":"string"}}});
        assert!(check_closed_schema(&closed, &closed).is_ok());
    }
}
