use crate::KnowledgeError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use veoveo_types::ServerSlug;

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

macro_rules! name {
    ($name:ident, $valid:expr, $message:literal, $pattern:literal, $max:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, KnowledgeError> {
                let value = value.into();
                if !($valid)(&value) { return Err(KnowledgeError($message)); }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str { &self.0 }
        }
        impl TryFrom<String> for $name {
            type Error = KnowledgeError;
            fn try_from(value: String) -> Result<Self, Self::Error> { Self::new(value) }
        }
        impl From<$name> for String { fn from(value: $name) -> Self { value.0 } }
        impl FromStr for $name {
            type Err = KnowledgeError;
            fn from_str(value: &str) -> Result<Self, Self::Err> { Self::new(value) }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
        }
        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> { stringify!($name).into() }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({"type": "string", "pattern": $pattern, "maxLength": $max, "minLength": 1})
            }
        }
    };
}

name!(
    CollectionName,
    slug,
    "invalid collection name",
    "^[a-z][a-z0-9-]*$",
    128
);
name!(
    DocumentId,
    slug,
    "invalid document id",
    "^[a-z][a-z0-9-]*$",
    128
);
name!(
    EntityKind,
    slug,
    "invalid entity kind",
    "^[a-z][a-z0-9-]*$",
    128
);
name!(
    ExternalSystemId,
    slug,
    "invalid external system id",
    "^[a-z][a-z0-9-]*$",
    128
);
name!(
    ExternalRecordId,
    |v: &str| !v.trim().is_empty() && v.len() <= 1024 && !v.chars().any(char::is_control),
    "external record id must contain 1 to 1024 bytes without controls",
    "^[^\\x00-\\x1f\\x7f]+$",
    1024
);
name!(
    Revision,
    |v: &str| !v.is_empty() && v.len() <= 256 && v.bytes().all(|b| (0x21..=0x7e).contains(&b)),
    "revision must contain 1 to 256 visible ASCII bytes",
    "^[!-~]+$",
    256
);

/// An installation-unique collection name owned by one server.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct CollectionId {
    server: ServerSlug,
    name: CollectionName,
}
impl CollectionId {
    pub fn new(server: ServerSlug, name: CollectionName) -> Result<Self, KnowledgeError> {
        if !slug(server.as_str()) {
            return Err(KnowledgeError("invalid collection server slug"));
        }
        Ok(Self { server, name })
    }
    pub fn server(&self) -> &ServerSlug {
        &self.server
    }
    pub fn name(&self) -> &CollectionName {
        &self.name
    }
}
impl TryFrom<String> for CollectionId {
    type Error = KnowledgeError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let (server, name) = value
            .split_once('.')
            .ok_or(KnowledgeError("collection must be server.name"))?;
        Self::new(
            ServerSlug::new(server).map_err(|_| KnowledgeError("invalid collection server"))?,
            name.parse()?,
        )
    }
}
impl From<CollectionId> for String {
    fn from(value: CollectionId) -> Self {
        value.to_string()
    }
}
impl FromStr for CollectionId {
    type Err = KnowledgeError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.to_owned().try_into()
    }
}
impl fmt::Display for CollectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.server, self.name)
    }
}
