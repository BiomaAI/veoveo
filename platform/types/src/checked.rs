//! Owner admission followed by immutable access to the admitted value.
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{borrow::Cow, ops::Deref};

/// A field-preserving check of value and field relationships.
/// Current authorization and external liveness belong to the service, not admission.
/// Normalization belongs in an explicit owner adapter.
pub trait Check {
    type Error;
    fn check(&self) -> Result<(), Self::Error>;
}

/// An admitted value. Owners must also prevent mutation through interior mutability.
/// ```compile_fail
/// use veoveo_types::{Check, Checked};
/// struct Value { count: usize }
/// impl Check for Value { type Error = &'static str; fn check(&self) -> Result<(), Self::Error> { Ok(()) } }
/// let mut value = Checked::new(Value { count: 1 }).unwrap();
/// value.count = 0;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Checked<T: Check>(T);
impl<T: Check> Checked<T> {
    pub fn new(value: T) -> Result<Self, T::Error> {
        value.check()?;
        Ok(Self(value))
    }
    pub fn get(&self) -> &T {
        &self.0
    }
    pub fn into_inner(self) -> T {
        self.0
    }
}
impl<T: Check> Deref for Checked<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.get()
    }
}
impl<T: Check + Serialize> Serialize for Checked<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl<'de, T: Check + Deserialize<'de>> Deserialize<'de> for Checked<T>
where
    T::Error: std::fmt::Display,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(T::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
impl<T: Check + JsonSchema> JsonSchema for Checked<T> {
    fn schema_name() -> Cow<'static, str> {
        T::schema_name()
    }
    fn schema_id() -> Cow<'static, str> {
        T::schema_id()
    }
    fn inline_schema() -> bool {
        T::inline_schema()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        T::json_schema(generator)
    }
}
