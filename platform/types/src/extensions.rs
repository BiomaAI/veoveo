//! Immutable owner-admitted contributions; registries carry no domain vocabulary.
use serde::{
    Serialize,
    de::{DeserializeOwned, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    fmt,
    marker::PhantomData,
    sync::Arc,
};

#[veoveo_types::id(text(ExtensionNames))]
pub struct ExtensionName(String);
#[doc(hidden)]
pub struct ExtensionNames;
impl crate::IdProfile for ExtensionNames {
    fn naming_profile(_: crate::IdMetadata) -> Option<crate::ScalarNaming> {
        Some(crate::ScalarNaming::builtin(
            crate::ScalarGrammar::ExtensionName,
        ))
    }
    type Error = ExtensionError;
    const PROFILE: crate::IdProfileSpec<Self::Error> =
        crate::IdProfileSpec::text(|value, _| validate_extension_name(value));
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ExtensionError(String);
impl ExtensionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
#[derive(Debug)]
struct RegistryIdentity;
type Payload = Arc<dyn Any + Send + Sync>;
type Admission = Arc<dyn Fn(Value) -> Result<(Value, Payload), ExtensionError> + Send + Sync>;
struct Codec {
    admit: Admission,
    type_id: std::any::TypeId,
}

/// A key cannot be manufactured or transferred to an unrelated registry.
pub struct ExtensionKey<T> {
    identity: Arc<RegistryIdentity>,
    name: ExtensionName,
    admit: Admission,
    marker: PhantomData<fn() -> T>,
}
impl<T> Clone for ExtensionKey<T> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            name: self.name.clone(),
            admit: self.admit.clone(),
            marker: PhantomData,
        }
    }
}
impl<T> fmt::Debug for ExtensionKey<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ExtensionKey").field(&self.name).finish()
    }
}

pub struct ExtensionRegistryBuilder {
    identity: Arc<RegistryIdentity>,
    core: BTreeSet<String>,
    codecs: BTreeMap<ExtensionName, Option<Codec>>,
}
impl ExtensionRegistryBuilder {
    pub fn new(core_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            identity: Arc::new(RegistryIdentity),
            core: core_names.into_iter().map(Into::into).collect(),
            codecs: BTreeMap::new(),
        }
    }
    pub fn reserve(&mut self, name: ExtensionName) -> Result<(), ExtensionError> {
        if self.core.contains(name.as_str()) {
            return Err(ExtensionError::new("extension collides with a core field"));
        }
        if self.codecs.contains_key(&name) {
            return Err(ExtensionError::new("extension already declared"));
        }
        self.codecs.insert(name, None);
        Ok(())
    }
    pub fn bind<T>(
        &mut self,
        name: &ExtensionName,
        admit: impl Fn(Value) -> Result<T, ExtensionError> + Send + Sync + 'static,
    ) -> Result<ExtensionKey<T>, ExtensionError>
    where
        T: Serialize + Send + Sync + 'static,
    {
        let slot = self
            .codecs
            .get_mut(name)
            .ok_or_else(|| ExtensionError::new("extension is not reserved"))?;
        if slot.is_some() {
            return Err(ExtensionError::new("extension codec already bound"));
        }
        *slot = Some(Codec {
            type_id: std::any::TypeId::of::<T>(),
            admit: Arc::new(move |value| {
                let typed = admit(value)?;
                let encoded = serde_json::to_value(&typed)
                    .map_err(|_| ExtensionError::new("extension serialization failed"))?;
                Ok((encoded, Arc::new(typed)))
            }),
        });
        Ok(ExtensionKey {
            identity: self.identity.clone(),
            name: name.clone(),
            admit: slot.as_ref().expect("codec assigned").admit.clone(),
            marker: PhantomData,
        })
    }
    pub fn bind_serde<T>(&mut self, name: &ExtensionName) -> Result<ExtensionKey<T>, ExtensionError>
    where
        T: Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.bind(name, |value| {
            serde_json::from_value(value)
                .map_err(|_| ExtensionError::new("extension admission failed"))
        })
    }
    pub fn build(self) -> ExtensionRegistry {
        ExtensionRegistry {
            identity: self.identity,
            codecs: Arc::new(self.codecs),
        }
    }
}
#[derive(Clone)]
pub struct ExtensionRegistry {
    identity: Arc<RegistryIdentity>,
    codecs: Arc<BTreeMap<ExtensionName, Option<Codec>>>,
}
impl Default for ExtensionRegistry {
    fn default() -> Self {
        ExtensionRegistryBuilder::new(std::iter::empty::<String>()).build()
    }
}
impl fmt::Debug for ExtensionRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionRegistry")
            .field("reserved", &self.codecs.keys().collect::<Vec<_>>())
            .finish()
    }
}
impl ExtensionRegistry {
    pub fn check(&self, values: &AdmittedExtensions) -> Result<(), ExtensionError> {
        if values
            .identity
            .as_ref()
            .is_some_and(|identity| Arc::ptr_eq(identity, &self.identity))
        {
            Ok(())
        } else {
            Err(ExtensionError::new(
                "contribution belongs to another registry or is unadmitted",
            ))
        }
    }

    pub fn reserved_names(&self) -> impl Iterator<Item = &ExtensionName> {
        self.codecs.keys()
    }
    pub fn key<T: Send + Sync + 'static>(
        &self,
        name: &ExtensionName,
    ) -> Result<ExtensionKey<T>, ExtensionError> {
        let codec = self
            .codecs
            .get(name)
            .and_then(Option::as_ref)
            .ok_or_else(|| ExtensionError::new("extension codec is not bound"))?;
        if codec.type_id != std::any::TypeId::of::<T>() {
            return Err(ExtensionError::new("extension key type mismatch"));
        }
        Ok(ExtensionKey {
            identity: self.identity.clone(),
            name: name.clone(),
            admit: codec.admit.clone(),
            marker: PhantomData,
        })
    }
    pub fn admit(
        &self,
        values: BTreeMap<String, Value>,
    ) -> Result<AdmittedExtensions, ExtensionError> {
        let mut admitted = AdmittedExtensions {
            identity: Some(self.identity.clone()),
            values: BTreeMap::new(),
        };
        for (name, value) in values {
            let Some((name, codec)) = self.codecs.iter().find(|(key, _)| key.as_str() == name)
            else {
                continue;
            };
            let codec = codec.as_ref().ok_or_else(|| {
                ExtensionError::new("signed reserved extension has no bound codec")
            })?;
            let (wire, _) = (codec.admit)(value)?;
            admitted
                .values
                .insert(name.clone(), AdmittedPayload { wire });
        }
        Ok(admitted)
    }
    pub fn contribute<T: Serialize>(
        &self,
        key: &ExtensionKey<T>,
        value: &T,
    ) -> Result<AdmittedExtensions, ExtensionError> {
        if !Arc::ptr_eq(&self.identity, &key.identity) {
            return Err(ExtensionError::new(
                "extension key belongs to another registry",
            ));
        }
        let value = serde_json::to_value(value)
            .map_err(|_| ExtensionError::new("extension serialization failed"))?;
        self.admit(BTreeMap::from([(key.name.0.clone(), value)]))
    }
}
#[derive(Clone)]
struct AdmittedPayload {
    wire: Value,
}
#[derive(Clone, Default)]
pub struct AdmittedExtensions {
    identity: Option<Arc<RegistryIdentity>>,
    values: BTreeMap<ExtensionName, AdmittedPayload>,
}
impl fmt::Debug for AdmittedExtensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdmittedExtensions")
            .field("names", &self.values.keys().collect::<Vec<_>>())
            .finish()
    }
}
impl PartialEq for AdmittedExtensions {
    fn eq(&self, other: &Self) -> bool {
        self.values.len() == other.values.len()
            && self
                .values
                .iter()
                .all(|(k, v)| other.values.get(k).is_some_and(|o| o.wire == v.wire))
    }
}
impl Eq for AdmittedExtensions {}
impl AdmittedExtensions {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn get<T: Send + Sync + 'static>(
        &self,
        key: &ExtensionKey<T>,
    ) -> Result<Option<T>, ExtensionError> {
        if let Some(identity) = &self.identity
            && !Arc::ptr_eq(identity, &key.identity)
        {
            return Err(ExtensionError::new(
                "extension key belongs to another registry",
            ));
        }
        self.values
            .get(&key.name)
            .map(|value| {
                let (wire, typed) = (key.admit)(value.wire.clone())?;
                if wire != value.wire {
                    return Err(ExtensionError::new(
                        "extension codec changed admitted representation",
                    ));
                }
                let typed = Arc::downcast::<T>(typed)
                    .map_err(|_| ExtensionError::new("extension key type mismatch"))?;
                Arc::try_unwrap(typed).map_err(|_| {
                    ExtensionError::new("extension codec did not yield an owned value")
                })
            })
            .transpose()
    }
    pub fn wire(&self) -> BTreeMap<String, Value> {
        self.values
            .iter()
            .map(|(name, payload)| (name.0.clone(), payload.wire.clone()))
            .collect()
    }
}

/// Parses an open JSON value while rejecting duplicate object fields at every depth.
/// Protocol adapters then deserialize their known model and admit contributions.
pub struct UniqueJsonValue(pub Value);
impl<'de> serde::Deserialize<'de> for UniqueJsonValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueJsonValue;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON without duplicate fields")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate JSON field"));
                    }
                    let value = map.next_value::<UniqueJsonValue>()?;
                    values.insert(key, value.0);
                }
                Ok(UniqueJsonValue(Value::Object(values)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<UniqueJsonValue>()? {
                    values.push(value.0);
                }
                Ok(UniqueJsonValue(Value::Array(values)))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| UniqueJsonValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("invalid JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(v.into()))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJsonValue(Value::Null))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

fn validate_extension_name(value: &str) -> Result<(), ExtensionError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b':'))
    {
        return Err(ExtensionError::new("invalid extension name"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Contribution {
        count: u64,
    }
    fn bound() -> (ExtensionRegistry, ExtensionKey<Contribution>) {
        let mut builder = ExtensionRegistryBuilder::new(["core"]);
        let name = ExtensionName::parse("example_count").unwrap();
        builder.reserve(name.clone()).unwrap();
        let key = builder.bind_serde(&name).unwrap();
        (builder.build(), key)
    }
    #[test]
    fn reserved_names_and_typed_keys_cannot_be_replaced_or_cross_registries() {
        let mut builder = ExtensionRegistryBuilder::new(["core"]);
        assert!(
            builder
                .reserve(ExtensionName::parse("core").unwrap())
                .is_err()
        );
        let name = ExtensionName::parse("example_count").unwrap();
        builder.reserve(name.clone()).unwrap();
        assert!(builder.reserve(name.clone()).is_err());
        builder.bind_serde::<Contribution>(&name).unwrap();
        assert!(builder.bind_serde::<Contribution>(&name).is_err());
        let (registry, key) = bound();
        let (other, other_key) = bound();
        let admitted = registry
            .contribute(&key, &Contribution { count: 7 })
            .unwrap();
        assert_eq!(admitted.get(&key).unwrap().unwrap().count, 7);
        assert!(admitted.get(&other_key).is_err());
        assert!(other.contribute(&key, &Contribution { count: 8 }).is_err());
        assert!(registry.key::<String>(&name).is_err());
        assert_eq!(
            admitted
                .get(&registry.key::<Contribution>(&name).unwrap())
                .unwrap()
                .unwrap()
                .count,
            7
        );
    }
    #[test]
    fn reserved_claims_require_codecs_and_valid_values_unrelated_claims_are_ignored() {
        let mut builder = ExtensionRegistryBuilder::new(std::iter::empty::<String>());
        builder
            .reserve(ExtensionName::parse("example_count").unwrap())
            .unwrap();
        let registry = builder.build();
        assert!(
            registry
                .admit(BTreeMap::from([("example_count".into(), Value::Null)]))
                .is_err()
        );
        assert!(
            registry
                .admit(BTreeMap::from([("unrelated".into(), Value::Null)]))
                .unwrap()
                .is_empty()
        );
        let (registry, key) = bound();
        assert!(
            registry
                .admit(BTreeMap::from([("example_count".into(), Value::Null)]))
                .is_err()
        );
        let admitted = registry
            .contribute(&key, &Contribution { count: 987654321 })
            .unwrap();
        assert!(!format!("{admitted:?}").contains("987654321"));
    }
    #[test]
    fn duplicate_json_fields_reject_before_open_values_can_overwrite() {
        for text in [
            r#"{"a":1,"a":2}"#,
            r#"{"extension":{"a":1,"a":2}}"#,
            r#"[{"a":1,"a":2}]"#,
        ] {
            assert!(serde_json::from_str::<UniqueJsonValue>(text).is_err());
        }
        assert!(
            serde_json::from_str::<UniqueJsonValue>(r#"{"extension":{"a":[null,true,1,"value"]}}"#)
                .is_ok()
        );
    }
}

#[cfg(test)]
mod immutable_payload_tests {
    use super::*;
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Interior {
        counter: std::sync::Mutex<u64>,
    }
    #[test]
    fn owned_typed_values_cannot_mutate_admitted_wire_or_later_reads() {
        let mut builder = ExtensionRegistryBuilder::new(std::iter::empty::<String>());
        let name = ExtensionName::parse("interior").unwrap();
        builder.reserve(name.clone()).unwrap();
        let key = builder.bind_serde::<Interior>(&name).unwrap();
        let registry = builder.build();
        let admitted = registry
            .contribute(
                &key,
                &Interior {
                    counter: std::sync::Mutex::new(7),
                },
            )
            .unwrap();
        let owned = admitted.get(&key).unwrap().unwrap();
        *owned.counter.lock().unwrap() = 9;
        assert_eq!(admitted.wire()["interior"]["counter"], 7);
        assert_eq!(
            *admitted.get(&key).unwrap().unwrap().counter.lock().unwrap(),
            7
        );
    }
}
