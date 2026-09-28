//! Opaque absolute resource references. Domain parsers validate route meaning.
use crate::{IdentifierError, ResourceScheme};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUri(String);

impl ResourceUri {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_reference(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validate a concrete hierarchical address and decode its components.
    /// Completion templates and historical opaque references may fail this check.
    pub fn components(&self) -> Result<crate::ResourceUriParts, crate::ResourceUriError> {
        crate::ResourceUriParts::parse(self.as_str())
    }
}

impl AsRef<str> for ResourceUri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ResourceUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ResourceUri {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUri> for String {
    fn from(value: ResourceUri) -> Self {
        value.0
    }
}

// TODO(foundations): qualify remaining opaque URI families before tightening this
// wire validator. Gateway templates now use ResourceTemplateUri; the gateway's v1
// audit adapter decodes old URI text independently of this type.
fn validate_reference(value: &str) -> Result<(), IdentifierError> {
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err(IdentifierError::new(
            value,
            "must be an absolute server-owned resource URI",
        ));
    };
    ResourceScheme::new(scheme)?;
    if rest.is_empty() || rest.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(IdentifierError::new(
            value,
            "must include a non-empty path and no whitespace/control characters",
        ));
    }
    Ok(())
}

/// A domain-owned typed address. Implementations validate their route and IDs;
/// a generic reference alone establishes neither a domain identity nor access.
/// Implementations may live in independent server libraries without core edits.
/// ```compile_fail
/// use veoveo_types::{ResourceAddress, ResourceUri};
/// fn domain_address<A: ResourceAddress>(_: A) {}
/// domain_address(ResourceUri::new("example://catalog").unwrap());
/// ```
pub trait ResourceAddress: Sized {
    type Error: std::error::Error + Send + Sync + 'static;

    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error>;
    fn to_uri(&self) -> Result<ResourceUri, Self::Error>;
}
