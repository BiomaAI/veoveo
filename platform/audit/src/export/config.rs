//! Checked, public destination configuration. Credentials come from provider
//! credential chains or an explicitly named environment variable, never this file.
use super::{ExportError, digest};
use crate::integrity::canonical_bytes;
use serde::{Deserialize, Serialize};
use std::{num::NonZeroU32, path::Path};
use url::Url;
use veoveo_audit_contract::AuditDestinationId;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditExportConfig {
    pub s3: Option<S3Config>,
    pub otlp: Option<OtlpConfig>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S3Config {
    pub endpoint: Url,
    pub region: String,
    pub bucket: BucketName,
    pub prefix: ObjectPrefix,
    pub object_lock: ObjectLock,
    #[serde(default)]
    pub allow_http: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectLock {
    Disabled,
    Compliance { days: NonZeroU32 },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OtlpConfig {
    /// Complete Logs URL, including /v1/logs.
    pub endpoint: Url,
    #[serde(default)]
    pub allow_http: bool,
    pub bearer_token_env: Option<String>,
}
fn valid_bucket(value: &str) -> bool {
    (3..=63).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric()
}
fn valid_prefix(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BucketName(String);
impl TryFrom<String> for BucketName {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if !valid_bucket(&value) {
            return Err("invalid BucketName");
        }
        Ok(Self(value))
    }
}
impl From<BucketName> for String {
    fn from(value: BucketName) -> Self {
        value.0
    }
}
impl BucketName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ObjectPrefix(String);
impl TryFrom<String> for ObjectPrefix {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if !valid_prefix(&value) {
            return Err("invalid ObjectPrefix");
        }
        Ok(Self(value))
    }
}
impl From<ObjectPrefix> for String {
    fn from(value: ObjectPrefix) -> Self {
        value.0
    }
}
impl ObjectPrefix {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub(super) fn endpoint(url: &Url, allow_http: bool) -> Result<(), ExportError> {
    if !(url.scheme() == "https" || allow_http && url.scheme() == "http")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ExportError::Configuration(
            "export URL requires an approved HTTP(S) endpoint without credentials, query or fragment",
        ));
    }
    Ok(())
}
impl AuditExportConfig {
    pub fn load(path: &Path) -> Result<Self, ExportError> {
        use std::io::Read;
        let file = std::fs::File::open(path)
            .map_err(|_| ExportError::Configuration("cannot open audit export configuration"))?;
        let mut bytes = Vec::new();
        file.take(65_537)
            .read_to_end(&mut bytes)
            .map_err(|_| ExportError::Configuration("cannot read audit export configuration"))?;
        if bytes.len() > 65_536 {
            return Err(ExportError::Configuration(
                "audit export configuration exceeds 64 KiB",
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| ExportError::Configuration("invalid audit export configuration"))
    }
}
pub(super) fn destination_id<T: Serialize>(
    kind: &'static str,
    config: &T,
) -> Result<AuditDestinationId, ExportError> {
    // The wire mapping version is part of identity; a format change cannot reuse
    // receipts or overwrite an object delivered by a different mapping.
    Ok(AuditDestinationId::from_configuration_hash(digest(
        &canonical_bytes(&("veoveo.audit.export/v1", kind, config))?,
    )))
}

#[cfg(test)]
mod destination_text_tests {
    use super::*;
    #[test]
    fn bucket_and_prefix_keep_distinct_lexical_profiles() {
        for bucket in ["abc", "a-1", &"a".repeat(63)] {
            assert_eq!(
                BucketName::try_from(bucket.to_owned()).unwrap().as_str(),
                bucket
            );
        }
        for invalid in ["ab", "Aaa", "abc.def", "-abc", "abc-", " abc"] {
            assert_eq!(
                BucketName::try_from(invalid.to_owned()).unwrap_err(),
                "invalid BucketName"
            );
        }
        for prefix in ["A", "a/B-c_d.ext", &"a".repeat(256)] {
            assert_eq!(
                ObjectPrefix::try_from(prefix.to_owned()).unwrap().as_str(),
                prefix
            );
        }
        for invalid in ["", "/a", "a/", "a//b", ".", "a/..", " a", &"a".repeat(257)] {
            assert_eq!(
                ObjectPrefix::try_from(invalid.to_owned()).unwrap_err(),
                "invalid ObjectPrefix"
            );
        }
    }
}
