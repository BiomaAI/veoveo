//! Signed block identities. Counters are decimal strings on the wire so JCS never
//! rounds a SurrealDB versionstamp through an IEEE-754 JSON number.
use crate::{AuditPartition, AuditRecordId};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use veoveo_types::Sha256Digest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid audit integrity value")]
pub struct AuditIntegrityValueError;
macro_rules! counter {
    ($name:ident, $minimum:expr) => {
        #[derive(
            Debug,
            Clone,
            Copy,
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
        pub struct $name(u64);
        impl $name {
            pub fn new(value: u64) -> Result<Self, AuditIntegrityValueError> {
                if !($minimum..=i64::MAX as u64).contains(&value) {
                    return Err(AuditIntegrityValueError);
                }
                Ok(Self(value))
            }
            pub fn get(self) -> u64 {
                self.0
            }
        }
        impl FromStr for $name {
            type Err = AuditIntegrityValueError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let number = value.parse::<u64>().map_err(|_| AuditIntegrityValueError)?;
                if number.to_string() != value {
                    return Err(AuditIntegrityValueError);
                }
                Self::new(number)
            }
        }
        impl TryFrom<String> for $name {
            type Error = AuditIntegrityValueError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                value.parse()
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> String {
                value.0.to_string()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
counter!(AuditBlockSequence, 1);
counter!(AuditVersionstamp, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditBlockMember {
    pub id: AuditRecordId,
    pub versionstamp: AuditVersionstamp,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AuditBlockSchema {
    #[serde(rename = "veoveo.ai/audit-block/v1")]
    V1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditBlockHead {
    pub schema: AuditBlockSchema,
    pub partition: AuditPartition,
    pub sequence: AuditBlockSequence,
    pub first_versionstamp: AuditVersionstamp,
    pub last_versionstamp: AuditVersionstamp,
    pub members: Vec<AuditBlockMember>,
    pub root: Sha256Digest,
    pub previous: Option<Sha256Digest>,
    /// SHA-256 of the raw Ed25519 public key identifies a dedicated audit key.
    pub key_id: Sha256Digest,
    pub sealed_at: DateTime<Utc>,
}

/// A signature has exactly 64 bytes, represented as lowercase hex in JSON.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct AuditSignature(String);
impl AuditSignature {
    pub fn from_bytes(value: [u8; 64]) -> Self {
        use fmt::Write;
        let mut hex = String::with_capacity(128);
        for byte in value {
            write!(hex, "{byte:02x}").expect("String write");
        }
        Self(hex)
    }
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut bytes = [0; 64];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&self.0[index * 2..index * 2 + 2], 16).expect("checked hex");
        }
        bytes
    }
}
impl fmt::Debug for AuditSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AuditSignature").field(&self.0).finish()
    }
}
impl TryFrom<String> for AuditSignature {
    type Error = AuditIntegrityValueError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() != 128
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AuditIntegrityValueError);
        }
        Ok(Self(value))
    }
}
impl From<AuditSignature> for String {
    fn from(value: AuditSignature) -> String {
        value.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditBlock {
    pub head: AuditBlockHead,
    pub head_hash: Sha256Digest,
    pub signature: AuditSignature,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditCheckpoint {
    pub partition: AuditPartition,
    pub sequence: AuditBlockSequence,
    pub head_hash: Sha256Digest,
    pub last_versionstamp: AuditVersionstamp,
}
impl AuditBlock {
    pub fn checkpoint(&self) -> AuditCheckpoint {
        AuditCheckpoint {
            partition: self.head.partition.clone(),
            sequence: self.head.sequence,
            head_hash: self.head_hash.clone(),
            last_versionstamp: self.head.last_versionstamp,
        }
    }
}
