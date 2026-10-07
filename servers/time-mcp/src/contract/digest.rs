//! Time's bare-hexadecimal wire adapter over the foundational SHA-256 value.
use std::{borrow::Cow, fmt, str::FromStr};

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use veoveo_types::Sha256Digest;

/// The existing admin wire profile accepts 64 hexadecimal digits in either case.
/// Equality preserves spelling for acquisition idempotency; compare `canonical()`
/// values when checking downloaded content.
/// ```compile_fail
/// use veoveo_time_mcp::AuthoritySourceDigest;
/// let digest: AuthoritySourceDigest = "a".repeat(64);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct AuthoritySourceDigest {
    wire_hex: String,
    canonical: Sha256Digest,
}

impl AuthoritySourceDigest {
    pub fn parse(value: impl Into<String>) -> Result<Self, AuthoritySourceDigestError> {
        let wire_hex = value.into();
        if wire_hex.len() != 64 {
            return Err(AuthoritySourceDigestError);
        }
        let canonical = Sha256Digest::from_hex(wire_hex.to_ascii_lowercase())
            .map_err(|_| AuthoritySourceDigestError)?;
        Ok(Self {
            wire_hex,
            canonical,
        })
    }

    pub fn from_canonical(canonical: Sha256Digest) -> Self {
        Self {
            wire_hex: canonical.hex().to_owned(),
            canonical,
        }
    }

    pub fn as_hex(&self) -> &str {
        &self.wire_hex
    }

    pub fn canonical(&self) -> &Sha256Digest {
        &self.canonical
    }
}

impl FromStr for AuthoritySourceDigest {
    type Err = AuthoritySourceDigestError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<String> for AuthoritySourceDigest {
    type Error = AuthoritySourceDigestError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<AuthoritySourceDigest> for String {
    fn from(value: AuthoritySourceDigest) -> Self {
        value.wire_hex
    }
}

impl fmt::Display for AuthoritySourceDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.wire_hex)
    }
}

impl JsonSchema for AuthoritySourceDigest {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> Cow<'static, str> {
        "AuthoritySourceDigest".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({ "type": "string", "minLength": 64, "maxLength": 64,
            "pattern": "^[0-9a-fA-F]{64}$" })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("authority source digest must contain 64 hexadecimal digits")]
pub struct AuthoritySourceDigestError;
