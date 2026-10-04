use std::fmt;

use schemars::{Schema, SchemaGenerator};

const SHA256_PREFIX: &str = "sha256:";
const SHA256_HEX_LENGTH: usize = 64;

/// Canonical SHA-256 digest used by cross-server provenance contracts.
#[veoveo_types::id(prefixed(DigestIds, "sha256:"))]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected sha256: followed by 64 lowercase hexadecimal digits")]
pub struct Sha256DigestError;

#[doc(hidden)]
pub struct DigestIds;
impl crate::IdProfile for DigestIds {
    type Error = Sha256DigestError;
    const PROFILE: crate::IdProfileSpec<Self::Error> = crate::IdProfileSpec {
        schema: crate::IdSchema::Owner {
            schema: digest_schema,
            inline: true,
        },
        ..crate::IdProfileSpec::hex(
            crate::HexGrammar {
                length: SHA256_HEX_LENGTH,
                case: crate::HexCase::Lower,
                nonzero: false,
            },
            |_, _, _| Sha256DigestError,
        )
    };
}
fn digest_schema(_: &mut SchemaGenerator, _: crate::IdMetadata) -> Schema {
    schemars::json_schema!({
        "type": "string",
        "pattern": "^sha256:[0-9a-f]{64}$"
    })
}
