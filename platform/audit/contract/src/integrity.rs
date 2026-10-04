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
fn check_counter(value: u64, minimum: u64) -> Result<(), AuditIntegrityValueError> {
    if !(minimum..=i64::MAX as u64).contains(&value) {
        return Err(AuditIntegrityValueError);
    }
    Ok(())
}
fn parse_counter(value: &str) -> Result<u64, AuditIntegrityValueError> {
    let number = value.parse::<u64>().map_err(|_| AuditIntegrityValueError)?;
    if number.to_string() != value {
        return Err(AuditIntegrityValueError);
    }
    Ok(number)
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct AuditBlockSequence(u64);
impl AuditBlockSequence {
    pub fn new(value: u64) -> Result<Self, AuditIntegrityValueError> {
        check_counter(value, 1)?;
        Ok(Self(value))
    }
    pub fn get(self) -> u64 {
        self.0
    }
}
impl FromStr for AuditBlockSequence {
    type Err = AuditIntegrityValueError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let number = parse_counter(value)?;
        Self::new(number)
    }
}
impl TryFrom<String> for AuditBlockSequence {
    type Error = AuditIntegrityValueError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl From<AuditBlockSequence> for String {
    fn from(value: AuditBlockSequence) -> String {
        value.0.to_string()
    }
}
impl fmt::Display for AuditBlockSequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct AuditVersionstamp(u64);
impl AuditVersionstamp {
    pub fn new(value: u64) -> Result<Self, AuditIntegrityValueError> {
        check_counter(value, 0)?;
        Ok(Self(value))
    }
    pub fn get(self) -> u64 {
        self.0
    }
}
impl FromStr for AuditVersionstamp {
    type Err = AuditIntegrityValueError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let number = parse_counter(value)?;
        Self::new(number)
    }
}
impl TryFrom<String> for AuditVersionstamp {
    type Error = AuditIntegrityValueError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl From<AuditVersionstamp> for String {
    fn from(value: AuditVersionstamp) -> String {
        value.0.to_string()
    }
}
impl fmt::Display for AuditVersionstamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

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

#[cfg(test)]
mod counter_tests {
    use super::*;
    #[test]
    fn decimal_counters_preserve_admission_and_string_wire() {
        assert!(AuditBlockSequence::new(0).is_err());
        assert_eq!(AuditVersionstamp::new(0).unwrap().get(), 0);
        for value in [1, 117300000000000001, i64::MAX as u64] {
            let block = AuditBlockSequence::new(value).unwrap();
            let stamp = AuditVersionstamp::new(value).unwrap();
            let expected = value.to_string();
            assert_eq!(block.to_string(), expected);
            assert_eq!(
                serde_json::to_value(block).unwrap(),
                serde_json::Value::String(expected.clone())
            );
            assert_eq!(
                serde_json::to_value(stamp).unwrap(),
                serde_json::Value::String(expected.clone())
            );
            assert_eq!(block.serialize(BinaryStringProbe).unwrap(), expected);
            assert_eq!(stamp.serialize(BinaryStringProbe).unwrap(), expected);
            assert_eq!(
                serde_json::from_value::<AuditVersionstamp>(serde_json::Value::String(expected))
                    .unwrap(),
                stamp
            );
        }
        for invalid in ["", "+1", "01", " 1", "1 ", "-1", "9223372036854775808"] {
            assert!(
                invalid.parse::<AuditBlockSequence>().is_err(),
                "{invalid:?}"
            );
            assert!(invalid.parse::<AuditVersionstamp>().is_err(), "{invalid:?}");
        }
        assert!(AuditBlockSequence::new(i64::MAX as u64 + 1).is_err());
        assert!(serde_json::from_str::<AuditVersionstamp>("1").is_err());
    }
    struct BinaryStringProbe;
    impl serde::Serializer for BinaryStringProbe {
        type Ok = String;
        type Error = serde::de::value::Error;
        type SerializeSeq = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeTuple = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeTupleStruct = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeTupleVariant = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeMap = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeStruct = serde::ser::Impossible<Self::Ok, Self::Error>;
        type SerializeStructVariant = serde::ser::Impossible<Self::Ok, Self::Error>;
        fn is_human_readable(&self) -> bool {
            false
        }
        fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
            Ok(value.to_owned())
        }
        fn serialize_bool(self, _: bool) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_i8(self, _: i8) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_i16(self, _: i16) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_i32(self, _: i32) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_i64(self, _: i64) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_i128(self, _: i128) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_u8(self, _: u8) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_u16(self, _: u16) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_u32(self, _: u32) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_u64(self, _: u64) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_u128(self, _: u128) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_f32(self, _: f32) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_f64(self, _: f64) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_char(self, _: char) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_bytes(self, _: &[u8]) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_unit_struct(self, _: &'static str) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_unit_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
        ) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_tuple_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleStruct, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_tuple_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleVariant, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStruct, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_struct_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStructVariant, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_some<T: serde::Serialize + ?Sized>(
            self,
            _: &T,
        ) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_newtype_struct<T: serde::Serialize + ?Sized>(
            self,
            _: &'static str,
            _: &T,
        ) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
        fn serialize_newtype_variant<T: serde::Serialize + ?Sized>(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: &T,
        ) -> Result<Self::Ok, Self::Error> {
            Err(serde::ser::Error::custom("expected binary string"))
        }
    }
}
