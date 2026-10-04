//! Shared lexical profiles for public identifiers. Domain newtypes select a profile.
//! These validators accept public text only; errors retain the rejected input.

use crate::IdentifierError;

/// Nonempty lowercase ASCII letters, digits, hyphens, or underscores.
/// This lexical profile does not establish a route or domain identity.
pub fn validate_path_id(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(
            value,
            "must not be empty and must contain lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

/// Nonempty text without whitespace or controls.
/// This repository profile accepts Unicode; it is broader than OAuth scope-token syntax.
pub fn validate_token_text(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    Ok(())
}

/// Nonempty text without controls, preserving whitespace and Unicode.
/// Parsing an identity claim does not authenticate its issuer or grant authority.
pub fn validate_claim_text(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(char::is_control) {
        return Err(IdentifierError::new(
            value,
            "must not contain control characters",
        ));
    }
    Ok(())
}

#[doc(hidden)]
pub struct PathIdProfile;
impl crate::IdProfile for PathIdProfile {
    type Error = IdentifierError;
    const PROFILE: crate::IdProfileSpec<Self::Error> =
        crate::IdProfileSpec::text(|value, _| validate_path_id(value));
}

#[doc(hidden)]
pub struct TokenTextProfile;
impl crate::IdProfile for TokenTextProfile {
    type Error = IdentifierError;
    const PROFILE: crate::IdProfileSpec<Self::Error> =
        crate::IdProfileSpec::text(|value, _| validate_token_text(value));
}

#[doc(hidden)]
pub struct ClaimTextProfile;
impl crate::IdProfile for ClaimTextProfile {
    type Error = IdentifierError;
    const PROFILE: crate::IdProfileSpec<Self::Error> =
        crate::IdProfileSpec::text(|value, _| validate_claim_text(value));
}
