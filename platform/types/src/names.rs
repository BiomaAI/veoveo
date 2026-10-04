use crate::IdentifierError;

#[doc = "OAuth/OIDC scope value. It must not contain whitespace or control characters."]
#[veoveo_types::id(text(crate::identifier_syntax::TokenTextProfile))]
pub struct ScopeName(String);
#[doc = "Server-owned resource URI scheme, for example `media`."]
#[veoveo_types::id(text(SchemeNames))]
pub struct ResourceScheme(String);

/// Implement this on a domain-owned closed scope enum. Core owns no variants.
/// The returned name must be compared with authenticated grants by the policy owner.
/// A validated dynamic name does not implement a domain vocabulary:
/// ```compile_fail
/// use veoveo_types::{ScopeDefinition, ScopeName};
/// fn domain_scope<S: ScopeDefinition>(_: S) {}
/// domain_scope(ScopeName::parse("example:read").unwrap());
/// ```
pub trait ScopeDefinition: Copy + Eq {
    fn name(self) -> &'static ScopeName;
}

#[doc(hidden)]
pub struct SchemeNames;
impl crate::IdProfile for SchemeNames {
    type Error = IdentifierError;
    const PROFILE: crate::IdProfileSpec<Self::Error> =
        crate::IdProfileSpec::text(|value, _| validate_scheme(value));
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
