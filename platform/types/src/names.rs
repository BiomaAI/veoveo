use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{IdentifierError, identifier_syntax::validate_token_text};

macro_rules! name {
    ($name:ident, $validate:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                $validate(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

pub(crate) use name;

name!(
    ScopeName,
    validate_token_text,
    "OAuth/OIDC scope value. It must not contain whitespace or control characters."
);
name!(
    ResourceScheme,
    validate_scheme,
    "Server-owned resource URI scheme, for example `media`."
);

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
