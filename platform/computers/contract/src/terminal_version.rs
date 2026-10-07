//! Current numeric terminal profile, shared by construction and wire admission.
use crate::ComputerResultError;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalVersion(u8);

impl TerminalVersion {
    pub const CURRENT: Self = Self(2);

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TerminalVersion {
    type Error = ComputerResultError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value == Self::CURRENT.0 {
            Ok(Self::CURRENT)
        } else {
            Err(ComputerResultError)
        }
    }
}

impl From<TerminalVersion> for u8 {
    fn from(value: TerminalVersion) -> Self {
        value.get()
    }
}

impl Serialize for TerminalVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for TerminalVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(u8::deserialize(deserializer)?)
            .map_err(|_| serde::de::Error::custom("unsupported Computer terminal version"))
    }
}

impl JsonSchema for TerminalVersion {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TerminalVersion".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = u8::json_schema(generator);
        schema.insert("minimum".into(), Self::CURRENT.get().into());
        schema.insert("maximum".into(), Self::CURRENT.get().into());
        schema
    }
}
