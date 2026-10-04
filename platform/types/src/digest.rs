use std::{borrow::Cow, error::Error, fmt};

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};

const SHA256_PREFIX: &str = "sha256:";
const SHA256_HEX_LENGTH: usize = 64;

/// Canonical SHA-256 digest used by cross-server provenance contracts.
#[derive(
    veoveo_types::Id, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, constructor = parse, error = Sha256DigestError, validate = validate_digest)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    /// Preserve the fixed-size output of a SHA-256 implementation without reparsing text.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        use fmt::Write as _;
        let mut value = String::with_capacity(SHA256_PREFIX.len() + SHA256_HEX_LENGTH);
        value.push_str(SHA256_PREFIX);
        for byte in bytes {
            write!(value, "{byte:02x}").expect("writing to a String is infallible");
        }
        Self(value)
    }

    pub fn from_hex(hex: impl AsRef<str>) -> Result<Self, Sha256DigestError> {
        Self::parse(format!("{SHA256_PREFIX}{}", hex.as_ref()))
    }

    pub fn hex(&self) -> &str {
        self.0
            .strip_prefix(SHA256_PREFIX)
            .expect("a Sha256Digest always has its canonical prefix")
    }
}

impl JsonSchema for Sha256Digest {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Sha256Digest")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^sha256:[0-9a-f]{64}$"
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sha256DigestError;

impl fmt::Display for Sha256DigestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("expected sha256: followed by 64 lowercase hexadecimal digits")
    }
}

impl Error for Sha256DigestError {}

fn validate_digest(value: &str) -> Result<(), Sha256DigestError> {
    let Some(hex) = value.strip_prefix(SHA256_PREFIX) else {
        return Err(Sha256DigestError);
    };
    if hex.len() != SHA256_HEX_LENGTH
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Sha256DigestError);
    }
    Ok(())
}
