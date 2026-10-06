//! Concrete absolute resource references. Domain parsers validate route meaning.
use crate::{ResourceScheme, ResourceUriError};
use iri_string::types::UriStr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUri(String);

impl ResourceUri {
    pub fn new(value: impl Into<String>) -> Result<Self, ResourceUriError> {
        let value = value.into();
        validate_reference(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validate a concrete hierarchical address and decode its components.
    /// Network references may use ports or other components outside this profile.
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
    type Error = ResourceUriError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUri> for String {
    fn from(value: ResourceUri) -> Self {
        value.0
    }
}

fn validate_reference(value: &str) -> Result<(), ResourceUriError> {
    // RFC 3986 parsing preserves concrete wire identity, including network ports
    // and fragments. Templates have their own type and never enter this parser.
    let uri = UriStr::new(value).map_err(|_| ResourceUriError::InvalidUri)?;
    ResourceScheme::parse(uri.scheme_str()).map_err(|_| ResourceUriError::InvalidUri)?;
    let authority = uri
        .authority_str()
        .ok_or(ResourceUriError::NotHierarchical)?;
    if authority.is_empty() && uri.path_str().is_empty() {
        return Err(ResourceUriError::InvalidUri);
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

/// A resource whose contents change with one native Task.
///
/// The domain owns this relationship and implements it only for Task-backed routes.
/// The returned identity establishes neither existence nor permission to observe it.
pub trait TaskResourceAddress: ResourceAddress {
    fn task_id(&self) -> crate::TaskId;
}

const _: () = {
    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[schemars(rename = "ResourceUri")]
    #[serde(try_from = "String", into = "String")]
    struct __NamingWire(String);
    impl JsonSchema for ResourceUri {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            <__NamingWire as JsonSchema>::schema_name()
        }
        fn schema_id() -> std::borrow::Cow<'static, str> {
            concat!(module_path!(), "::ResourceUri").into()
        }
        fn inline_schema() -> bool {
            <__NamingWire as JsonSchema>::inline_schema()
        }
        fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
            let schema = <__NamingWire as JsonSchema>::json_schema(generator);
            crate::naming::static_identity_schema(
                schema,
                Some(crate::ScalarNaming::builtin(
                    crate::ScalarGrammar::ResourceUri,
                )),
                module_path!(),
                "ResourceUri",
                generator,
            )
        }
    }
};
