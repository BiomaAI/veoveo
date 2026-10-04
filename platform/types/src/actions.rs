//! Registry-bound execution names for owner-declared closed vocabularies.
use crate::{ExtensionError, Vocabulary};

use std::{any::TypeId, collections::BTreeMap, fmt, marker::PhantomData, sync::Arc};

#[veoveo_types::id(text(ActionNames))]
pub struct ActionName(String);
#[doc(hidden)]
pub struct ActionNames;
impl crate::IdProfile for ActionNames {
    type Error = ExtensionError;
    const PROFILE: crate::IdProfileSpec<Self::Error> =
        crate::IdProfileSpec::text(|value, _| validate_action_name(value));
}
#[derive(Debug)]
struct Identity;

/// A runtime action can only originate from an admitted vocabulary or name resolution.
#[derive(Clone)]
pub struct ActionHandle {
    identity: Arc<Identity>,
    name: ActionName,
}
impl ActionHandle {
    pub fn name(&self) -> &ActionName {
        &self.name
    }
}
impl fmt::Debug for ActionHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ActionHandle").field(&self.name).finish()
    }
}
impl PartialEq for ActionHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity) && self.name == other.name
    }
}
impl Eq for ActionHandle {}

pub struct ActionKey<A> {
    identity: Arc<Identity>,
    names: Arc<BTreeMap<&'static str, ActionName>>,
    marker: PhantomData<fn() -> A>,
}
impl<A> Clone for ActionKey<A> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            names: self.names.clone(),
            marker: PhantomData,
        }
    }
}
impl<A> fmt::Debug for ActionKey<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionKey")
            .field("names", &self.names)
            .finish()
    }
}
impl<A: Vocabulary> ActionKey<A> {
    pub fn value(&self, action: &ActionHandle) -> Result<A, ExtensionError> {
        if !Arc::ptr_eq(&self.identity, &action.identity)
            || !self.names.values().any(|name| name == &action.name)
        {
            return Err(ExtensionError::new(
                "action does not belong to this vocabulary binding",
            ));
        }
        A::from_wire(action.name.as_str()).ok_or_else(|| {
            ExtensionError::new("action vocabulary no longer admits its declaration")
        })
    }
    pub fn action(&self, action: A) -> Result<ActionHandle, ExtensionError> {
        let name = self.names.get(action.as_str()).ok_or_else(|| {
            ExtensionError::new("action is not a member of its registered vocabulary")
        })?;
        Ok(ActionHandle {
            identity: self.identity.clone(),
            name: name.clone(),
        })
    }
}

pub struct ActionRegistryBuilder {
    identity: Arc<Identity>,
    names: BTreeMap<ActionName, Option<TypeId>>,
    vocabularies: BTreeMap<TypeId, Arc<BTreeMap<&'static str, ActionName>>>,
}
impl Default for ActionRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}
impl ActionRegistryBuilder {
    pub fn new() -> Self {
        Self {
            identity: Arc::new(Identity),
            names: BTreeMap::new(),
            vocabularies: BTreeMap::new(),
        }
    }
    pub fn reserve(&mut self, name: ActionName) -> Result<(), ExtensionError> {
        if self.names.contains_key(&name) {
            return Err(ExtensionError::new("action already declared"));
        }
        self.names.insert(name, None);
        Ok(())
    }
    pub fn register<A: Vocabulary>(&mut self) -> Result<ActionKey<A>, ExtensionError> {
        let type_id = TypeId::of::<A>();
        if self.vocabularies.contains_key(&type_id) {
            return Err(ExtensionError::new("action vocabulary already bound"));
        }
        if A::ALL.is_empty() {
            return Err(ExtensionError::new("action vocabulary is empty"));
        }
        let mut names = BTreeMap::new();
        for action in A::ALL {
            let name = ActionName::parse(action.as_str())?;
            if self.names.contains_key(&name) || names.values().any(|old| old == &name) {
                return Err(ExtensionError::new("action already declared"));
            }
            names.insert(action.as_str(), name);
        }
        for name in names.values() {
            self.names.insert(name.clone(), Some(type_id));
        }
        let names = Arc::new(names);
        self.vocabularies.insert(type_id, names.clone());
        Ok(ActionKey {
            identity: self.identity.clone(),
            names,
            marker: PhantomData,
        })
    }
    pub fn bind<A: Vocabulary>(&mut self) -> Result<ActionKey<A>, ExtensionError> {
        let type_id = TypeId::of::<A>();
        if A::ALL.is_empty() || self.vocabularies.contains_key(&type_id) {
            return Err(ExtensionError::new(
                "action vocabulary is empty or already bound",
            ));
        }
        let mut names = BTreeMap::new();
        for action in A::ALL {
            let name = ActionName::parse(action.as_str())?;
            if !matches!(self.names.get(&name), Some(None))
                || names.values().any(|old| old == &name)
            {
                return Err(ExtensionError::new(
                    "action binding requires unique reserved unbound names",
                ));
            }
            names.insert(action.as_str(), name);
        }
        for name in names.values() {
            self.names.insert(name.clone(), Some(type_id));
        }
        let names = Arc::new(names);
        self.vocabularies.insert(type_id, names.clone());
        Ok(ActionKey {
            identity: self.identity.clone(),
            names,
            marker: PhantomData,
        })
    }
    pub fn build(self) -> ActionRegistry {
        ActionRegistry {
            identity: self.identity,
            names: self.names,
            vocabularies: self.vocabularies,
        }
    }
}
#[derive(Clone)]
pub struct ActionRegistry {
    identity: Arc<Identity>,
    names: BTreeMap<ActionName, Option<TypeId>>,
    vocabularies: BTreeMap<TypeId, Arc<BTreeMap<&'static str, ActionName>>>,
}
impl fmt::Debug for ActionRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionRegistry")
            .field("names", &self.names.keys())
            .finish()
    }
}
impl ActionRegistry {
    pub fn names(&self) -> impl Iterator<Item = &ActionName> {
        self.names
            .iter()
            .filter_map(|(name, binding)| binding.as_ref().map(|_| name))
    }
    pub fn declared_names(&self) -> impl Iterator<Item = &ActionName> {
        self.names.keys()
    }
    pub fn resolve(&self, name: &ActionName) -> Result<ActionHandle, ExtensionError> {
        match self.names.get(name) {
            Some(Some(_)) => Ok(ActionHandle {
                identity: self.identity.clone(),
                name: name.clone(),
            }),
            Some(None) => Err(ExtensionError::new("action declaration is unbound")),
            None => Err(ExtensionError::new("unknown action")),
        }
    }
    pub fn key<A: Vocabulary>(&self) -> Result<ActionKey<A>, ExtensionError> {
        let names = self
            .vocabularies
            .get(&TypeId::of::<A>())
            .ok_or_else(|| ExtensionError::new("action vocabulary is unbound"))?;
        Ok(ActionKey {
            identity: self.identity.clone(),
            names: names.clone(),
            marker: PhantomData,
        })
    }
    pub fn check(&self, action: &ActionHandle) -> Result<(), ExtensionError> {
        if !Arc::ptr_eq(&self.identity, &action.identity) {
            return Err(ExtensionError::new("action belongs to another registry"));
        }
        self.resolve(&action.name).map(|_| ())
    }
    pub fn same_binding(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
    }
}

fn validate_action_name(value: &str) -> Result<(), ExtensionError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
        })
        || !value.as_bytes()[0].is_ascii_lowercase()
    {
        return Err(ExtensionError::new("invalid action name"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Vocabulary)]
    enum Fruit {
        Apple,
        Pear,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Vocabulary)]
    enum Collision {
        Apple,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Manual {
        Present,
        Missing,
    }
    impl Vocabulary for Manual {
        const ALL: &'static [Self] = &[Self::Present];
        const NAME: &'static str = "Manual";
        fn as_str(self) -> &'static str {
            match self {
                Self::Present => "present",
                Self::Missing => "missing",
            }
        }
    }
    #[test]
    fn immutable_keys_reject_foreign_family_and_incomplete_open_vocabulary() {
        let mut builder = ActionRegistryBuilder::new();
        let key = builder.register::<Fruit>().unwrap();
        let manual = builder.register::<Manual>().unwrap();
        assert!(manual.action(Manual::Missing).is_err());
        assert!(builder.register::<Fruit>().is_err());
        assert!(builder.register::<Collision>().is_err());
        let registry = builder.build();
        let apple = key.action(Fruit::Apple).unwrap();
        assert_eq!(key.value(&apple).unwrap(), Fruit::Apple);
        assert!(manual.value(&apple).is_err());
        let mut other = ActionRegistryBuilder::new();
        let other_key = other.register::<Fruit>().unwrap();
        assert!(
            registry
                .check(&other_key.action(Fruit::Apple).unwrap())
                .is_err()
        );
        assert!(other_key.value(&apple).is_err());
        assert!(
            registry
                .resolve(&ActionName::parse("unknown").unwrap())
                .is_err()
        );
    }
    #[test]
    fn reservation_and_binding_are_atomic_and_unbound_names_are_not_choices() {
        let mut builder = ActionRegistryBuilder::new();
        builder
            .reserve(ActionName::parse("apple").unwrap())
            .unwrap();
        assert!(builder.bind::<Fruit>().is_err());
        builder.reserve(ActionName::parse("pear").unwrap()).unwrap();
        let key = builder.bind::<Fruit>().unwrap();
        builder
            .reserve(ActionName::parse("unavailable").unwrap())
            .unwrap();
        let registry = builder.build();
        assert!(
            registry
                .resolve(&ActionName::parse("unavailable").unwrap())
                .is_err()
        );
        assert_eq!(
            registry.names().map(ActionName::as_str).collect::<Vec<_>>(),
            ["apple", "pear"]
        );
        registry.check(&key.action(Fruit::Pear).unwrap()).unwrap();
    }
}
