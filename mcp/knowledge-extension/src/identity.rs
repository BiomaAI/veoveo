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

#[veoveo_types::id(text(CollectionNameProfile), error_context = "invalid collection name")]
pub struct CollectionName(String);

#[veoveo_types::id(text(CollectionNameProfile), error_context = "invalid document id")]
pub struct DocumentId(String);

#[veoveo_types::id(text(CollectionNameProfile), error_context = "invalid entity kind")]
pub struct EntityKind(String);

#[veoveo_types::id(
    text(CollectionNameProfile),
    error_context = "invalid external system id"
)]
pub struct ExternalSystemId(String);

#[veoveo_types::id(text(ExternalRecordIdProfile))]
pub struct ExternalRecordId(String);

#[veoveo_types::id(text(RevisionProfile))]
pub struct Revision(String);

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
            ServerSlug::parse(server).map_err(|_| KnowledgeError("invalid collection server"))?,
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

fn knowledge_name_schema(pattern: &str, max: usize) -> schemars::Schema {
    schemars::json_schema!({"type":"string","pattern":pattern,"maxLength":max,"minLength":1})
}

use veoveo_types::{IdProfile, IdProfileSpec};

#[doc(hidden)]
pub struct CollectionNameProfile;
impl IdProfile for CollectionNameProfile {
    type Error = KnowledgeError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, metadata| {
        validate_knowledge_name(value, slug, metadata.error_context)
    })
    .owner_schema(
        |_, _| knowledge_name_schema("^[a-z][a-z0-9-]*$", 128),
        false,
    );
}

#[doc(hidden)]
pub struct ExternalRecordIdProfile;
impl IdProfile for ExternalRecordIdProfile {
    type Error = KnowledgeError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
        validate_knowledge_name(
            value,
            |v: &str| !v.trim().is_empty() && v.len() <= 1024 && !v.chars().any(char::is_control),
            "external record id must contain 1 to 1024 bytes without controls",
        )
    })
    .owner_schema(
        |_, _| knowledge_name_schema("^[^\\x00-\\x1f\\x7f]+$", 1024),
        false,
    );
}
#[doc(hidden)]
pub struct RevisionProfile;
impl IdProfile for RevisionProfile {
    type Error = KnowledgeError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
        validate_knowledge_name(
            value,
            |v: &str| {
                !v.is_empty() && v.len() <= 256 && v.bytes().all(|b| (0x21..=0x7e).contains(&b))
            },
            "revision must contain 1 to 256 visible ASCII bytes",
        )
    })
    .owner_schema(|_, _| knowledge_name_schema("^[!-~]+$", 256), false);
}

fn validate_knowledge_name(
    value: &str,
    valid: impl FnOnce(&str) -> bool,
    message: &'static str,
) -> Result<(), KnowledgeError> {
    if !valid(value) {
        return Err(KnowledgeError(message));
    }
    Ok(())
}
