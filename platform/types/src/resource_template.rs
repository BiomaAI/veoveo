//! RFC 6570 resource declarations, distinct from concrete resource addresses.
use std::{collections::BTreeMap, error::Error, fmt};

use iri_string::{
    spec::UriSpec,
    template::{Context, UriTemplateStr, simple_context::SimpleContext},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ResourceScheme, ResourceUri, ResourceUriError, ResourceUriParts};

/// An ASCII RFC 6570 template with a literal lowercase resource scheme and `://` prefix.
/// Validation preserves spelling and does not establish route ownership or policy.
/// A literal-only template is valid under RFC 6570.
/// ```compile_fail
/// use veoveo_types::{ResourceTemplateUri, ResourceUri};
/// fn read_concrete(_: ResourceUri) {}
/// read_concrete(ResourceTemplateUri::new("example://items/{id}").unwrap());
/// ```
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceTemplateUri(String);

impl ResourceTemplateUri {
    pub fn new(value: impl Into<String>) -> Result<Self, ResourceTemplateError> {
        let value = value.into();
        let (scheme, suffix) = value
            .split_once("://")
            .ok_or(ResourceTemplateError::InvalidAbsoluteReference)?;
        ResourceScheme::parse(scheme)
            .map_err(|_| ResourceTemplateError::InvalidAbsoluteReference)?;
        if suffix.is_empty() {
            return Err(ResourceTemplateError::InvalidAbsoluteReference);
        }
        let template =
            UriTemplateStr::new(&value).map_err(|_| ResourceTemplateError::InvalidSyntax)?;
        validate_prefix_lengths(template)?;
        // The pinned parser also admits empty dotted components in variable names.
        // Check through its variable iterator only after prefix bounds are safe.
        if template
            .variables()
            .any(|name| name.as_str().ends_with('.') || name.as_str().contains(".."))
        {
            return Err(ResourceTemplateError::InvalidSyntax);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Names follow RFC 6570 spelling, including percent-encoded names. Repeated
    /// occurrences are preserved; this is neither decoding nor a policy matcher.
    pub fn variables(&self) -> impl Iterator<Item = &str> {
        self.template().variables().map(|name| name.as_str())
    }

    /// Expand scalar variables without requiring a URI-library context in the
    /// consumer. Names use RFC 6570 spelling; unreferenced entries are ignored and
    /// missing variables remain undefined. Owners still validate required IDs and
    /// relationships with their domain address type.
    pub fn expand_scalars(
        &self,
        variables: &BTreeMap<String, String>,
    ) -> Result<ResourceUri, ResourceTemplateError> {
        let mut context = SimpleContext::new();
        for name in self.variables() {
            if let Some(value) = variables.get(name) {
                context.insert(name, value.clone());
            }
        }
        self.expand(&context)
    }

    /// Expand an upstream library context and validate the concrete resource profile.
    /// Domain constructors retain typed required inputs; RFC 6570 itself permits
    /// undefined variables. The result still needs its owner's route/ID validation.
    pub fn expand<C: Context>(&self, context: &C) -> Result<ResourceUri, ResourceTemplateError> {
        let expanded = self
            .template()
            .expand::<UriSpec, _>(context)
            .map_err(|_| ResourceTemplateError::Expansion)?
            .to_string();
        ResourceUriParts::parse(&expanded)
            .map(ResourceUriParts::into_uri)
            .map_err(ResourceTemplateError::Concrete)
    }

    fn template(&self) -> &UriTemplateStr {
        UriTemplateStr::new(&self.0).expect("template admission validated the private value")
    }
}

fn validate_prefix_lengths(template: &UriTemplateStr) -> Result<(), ResourceTemplateError> {
    // iri-string 0.7.14 accepts zero/leading-zero and five-digit prefix lengths.
    // Its expansion parser can also overflow u16 for an admitted five-digit value.
    // The upstream parser establishes expression structure first; this guard only
    // checks RFC 6570 section 2.4.1's max-length ABNF. Remove it when a qualified
    // upstream release rejects the regression cases in resource_templates.rs.
    for expression in template.as_str().split('{').skip(1) {
        let body = expression.split_once('}').expect("validated expression").0;
        for variable in body.split(',') {
            if let Some((_, length)) = variable.split_once(':')
                && (length.len() > 4 || length.starts_with('0'))
            {
                return Err(ResourceTemplateError::InvalidSyntax);
            }
        }
    }
    Ok(())
}

impl AsRef<str> for ResourceTemplateUri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for ResourceTemplateUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl TryFrom<String> for ResourceTemplateUri {
    type Error = ResourceTemplateError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<ResourceTemplateUri> for String {
    fn from(value: ResourceTemplateUri) -> Self {
        value.0
    }
}

/// Redacted failure categories; no template, binding value or upstream error is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceTemplateError {
    InvalidAbsoluteReference,
    InvalidSyntax,
    Expansion,
    Concrete(ResourceUriError),
}
impl fmt::Display for ResourceTemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAbsoluteReference => f.write_str("resource template requires a literal lowercase scheme and nonempty absolute suffix"),
            Self::InvalidSyntax => f.write_str("invalid resource template for the ASCII RFC 6570 profile"),
            Self::Expansion => f.write_str("resource template expansion failed"),
            Self::Concrete(error) => error.fmt(f),
        }
    }
}
impl Error for ResourceTemplateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Concrete(error) => Some(error),
            _ => None,
        }
    }
}
