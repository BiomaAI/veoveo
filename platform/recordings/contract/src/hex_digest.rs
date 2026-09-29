//! Bare lowercase hexadecimal at Recording fields; foundational digests internally.

use serde::{Deserialize, Deserializer, Serializer};
use veoveo_types::Sha256Digest;

pub fn serialize<S: Serializer>(digest: &Sha256Digest, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(digest.hex())
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Sha256Digest, D::Error> {
    Sha256Digest::from_hex(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}
