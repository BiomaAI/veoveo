use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{IdentifierError, identifier_syntax::validate_token_text};

#[doc = "OAuth/OIDC scope value. It must not contain whitespace or control characters."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_token_text)]
pub struct ScopeName(String);
#[doc = "Server-owned resource URI scheme, for example `media`."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_scheme)]
pub struct ResourceScheme(String);

/// Implement this on a domain-owned closed scope enum. Core owns no variants.
/// The returned name must be compared with authenticated grants by the policy owner.
/// A validated dynamic name does not implement a domain vocabulary:
/// ```compile_fail
/// use veoveo_types::{ScopeDefinition, ScopeName};
/// fn domain_scope<S: ScopeDefinition>(_: S) {}
/// domain_scope(ScopeName::new("example:read").unwrap());
/// ```
pub trait ScopeDefinition: Copy + Eq {
    fn name(self) -> &'static ScopeName;
}

fn validate_scheme(value: &str) -> Result<(), IdentifierError> {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return Err(IdentifierError::new(value, "must not be empty"));
    };
    if !first.is_ascii_lowercase() {
        return Err(IdentifierError::new(
            value,
            "must start with a lowercase ASCII letter",
        ));
    }
    if !bytes
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.'))
    {
        return Err(IdentifierError::new(
            value,
            "must follow URI scheme syntax with lowercase ASCII characters",
        ));
    }
    Ok(())
}
