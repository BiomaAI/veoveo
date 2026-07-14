use std::{collections::BTreeSet, fmt};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;

use super::{DatasetReleaseId, MapDatasetId, MapFamily, MapSourceId, Wgs84BoundingBox};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityClass {
    Regulator,
    InfrastructureOperator,
    HydrographicOffice,
    AirNavigationServiceProvider,
    TransportOperator,
    CommercialProvider,
    Community,
    Derived,
    SyntheticTest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionModel {
    Snapshot,
    SequencedDelta,
    EffectiveEvent,
    ObservationStream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceAdapterKind {
    OpenStreetMap,
    AuthorityVector,
    GtfsSchedule,
    GtfsRealtime,
    S57Enc,
    S100,
    Aixm,
    FaaNasr,
    Environmental,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct SecretReference(String);

impl SecretReference {
    pub fn parse(value: impl Into<String>) -> Result<Self, SourceContractError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || value == "."
            || value == ".."
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(SourceContractError::InvalidSecretReference);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SecretReference {
    type Error = SourceContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<SecretReference> for String {
    fn from(value: SecretReference) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceCredential {
    Bearer {
        secret_ref: SecretReference,
    },
    Header {
        secret_ref: SecretReference,
        header_name: SourceHeaderName,
    },
}

impl SourceCredential {
    pub fn secret_ref(&self) -> &SecretReference {
        match self {
            Self::Bearer { secret_ref } | Self::Header { secret_ref, .. } => secret_ref,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct SourceHeaderName(String);

impl SourceHeaderName {
    pub fn parse(value: impl Into<String>) -> Result<Self, SourceContractError> {
        let value = value.into().to_ascii_lowercase();
        if value.len() < 3
            || value.len() > 128
            || !value.starts_with("x-")
            || value.starts_with("x-forwarded-")
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(SourceContractError::InvalidCredentialHeader);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SourceHeaderName {
    type Error = SourceContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<SourceHeaderName> for String {
    fn from(value: SourceHeaderName) -> Self {
        value.0
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct HttpsEndpoint(String);

impl HttpsEndpoint {
    pub fn parse(value: impl Into<String>) -> Result<Self, SourceContractError> {
        let value = value.into();
        let parsed = Url::parse(&value).map_err(|_| SourceContractError::InvalidEndpoint)?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || parsed.username() != ""
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(SourceContractError::InvalidEndpoint);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn host(&self) -> String {
        Url::parse(&self.0)
            .expect("validated endpoint")
            .host_str()
            .expect("validated endpoint host")
            .to_owned()
    }
}

impl TryFrom<String> for HttpsEndpoint {
    type Error = SourceContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<HttpsEndpoint> for String {
    fn from(value: HttpsEndpoint) -> Self {
        value.0
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct SourceHost(String);

impl SourceHost {
    pub fn parse(value: impl Into<String>) -> Result<Self, SourceContractError> {
        let value = value.into().to_ascii_lowercase();
        if value.is_empty()
            || value.len() > 253
            || value.starts_with('.')
            || value.ends_with('.')
            || value.chars().any(|character| {
                !(character.is_ascii_alphanumeric() || character == '-' || character == '.')
            })
        {
            return Err(SourceContractError::InvalidHost);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SourceHost {
    type Error = SourceContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<SourceHost> for String {
    fn from(value: SourceHost) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MountRelativePath(String);

impl MountRelativePath {
    pub fn parse(value: impl Into<String>) -> Result<Self, SourceContractError> {
        let value = value.into();
        let path = std::path::Path::new(&value);
        if value.is_empty()
            || value.len() > 1_024
            || path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(SourceContractError::InvalidMountPath);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for MountRelativePath {
    type Error = SourceContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<MountRelativePath> for String {
    fn from(value: MountRelativePath) -> Self {
        value.0
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct SourceMountId(String);

impl SourceMountId {
    pub fn parse(value: impl Into<String>) -> Result<Self, SourceContractError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(SourceContractError::InvalidMountId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SourceMountId {
    type Error = SourceContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<SourceMountId> for String {
    fn from(value: SourceMountId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceLocation {
    Https {
        endpoint: HttpsEndpoint,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        allowed_redirect_hosts: BTreeSet<SourceHost>,
    },
    OsmReplication {
        snapshot_endpoint: HttpsEndpoint,
        replication_endpoint: HttpsEndpoint,
    },
    MountedExchangeSet {
        mount_id: SourceMountId,
        relative_path: MountRelativePath,
    },
}

impl SourceLocation {
    pub fn validate(&self) -> Result<(), SourceContractError> {
        match self {
            Self::Https {
                endpoint,
                allowed_redirect_hosts,
            } => {
                if !allowed_redirect_hosts.is_empty()
                    && !allowed_redirect_hosts
                        .iter()
                        .any(|host| host.as_str() == endpoint.host())
                {
                    return Err(SourceContractError::EndpointHostNotAllowed);
                }
                Ok(())
            }
            Self::OsmReplication {
                snapshot_endpoint,
                replication_endpoint,
            } => {
                if snapshot_endpoint.host().is_empty() || replication_endpoint.host().is_empty() {
                    return Err(SourceContractError::InvalidEndpoint);
                }
                Ok(())
            }
            Self::MountedExchangeSet { .. } => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DatasetLicense {
    pub license_id: String,
    pub source_terms_uri: HttpsEndpoint,
    pub attribution: String,
    pub redistribution_allowed: bool,
    pub derivatives_allowed: bool,
    pub offline_bundle_allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl DatasetLicense {
    pub fn validate(&self) -> Result<(), SourceContractError> {
        validate_controlled(&self.license_id, 128)?;
        if self.attribution.is_empty()
            || self.attribution.len() > 4_096
            || self.attribution.chars().any(char::is_control)
        {
            return Err(SourceContractError::InvalidAttribution);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegisteredSource {
    pub source_id: MapSourceId,
    pub dataset_id: MapDatasetId,
    pub name: String,
    pub adapter_kind: SourceAdapterKind,
    pub authority: AuthorityClass,
    pub acquisition_model: AcquisitionModel,
    pub map_families: BTreeSet<MapFamily>,
    pub location: SourceLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<SourceCredential>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub publisher_key_refs: BTreeSet<SecretReference>,
    pub expected_media_types: BTreeSet<String>,
    pub maximum_download_bytes: u64,
    pub maximum_elapsed_seconds: u64,
    pub license: DatasetLicense,
    pub enabled: bool,
    pub record_version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RegisteredSource {
    pub fn validate(&self) -> Result<(), SourceContractError> {
        validate_controlled(&self.name, 256)?;
        if self.map_families.is_empty()
            || self.expected_media_types.is_empty()
            || self.maximum_download_bytes == 0
            || !(1..=86_400).contains(&self.maximum_elapsed_seconds)
            || self.record_version == 0
            || self.updated_at < self.created_at
        {
            return Err(SourceContractError::InvalidSource);
        }
        self.location.validate()?;
        self.license.validate()?;
        self.expected_media_types
            .iter()
            .try_for_each(|value| validate_media_type(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DatasetReleaseState {
    Staged,
    Active,
    Retired,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DatasetRelease {
    pub release_id: DatasetReleaseId,
    pub dataset_id: MapDatasetId,
    pub source_id: MapSourceId,
    pub version_label: String,
    pub source_digest_sha256: String,
    pub coverage: Wgs84BoundingBox,
    pub acquired_at: DateTime<Utc>,
    pub valid_from: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    pub schema_version: u64,
    pub normalization_pipeline_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_build_version: Option<String>,
    pub license: DatasetLicense,
    pub raw_artifact_uri: String,
    pub normalized_artifact_uris: Vec<String>,
    pub quality_report_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_release_id: Option<DatasetReleaseId>,
    pub state: DatasetReleaseState,
    pub record_version: u64,
    pub updated_at: DateTime<Utc>,
}

impl DatasetRelease {
    pub fn validate(&self) -> Result<(), SourceContractError> {
        validate_controlled(&self.version_label, 256)?;
        validate_sha256(&self.source_digest_sha256)?;
        self.coverage
            .validate()
            .map_err(|_| SourceContractError::InvalidCoverage)?;
        if self
            .valid_until
            .is_some_and(|until| until <= self.valid_from)
            || self.schema_version == 0
            || self.record_version == 0
            || self.normalized_artifact_uris.is_empty()
        {
            return Err(SourceContractError::InvalidRelease);
        }
        validate_controlled(&self.normalization_pipeline_version, 256)?;
        validate_optional_controlled(self.routing_build_version.as_deref(), 256)?;
        validate_artifact_uri(&self.raw_artifact_uri)?;
        self.normalized_artifact_uris
            .iter()
            .try_for_each(|uri| validate_artifact_uri(uri))?;
        validate_artifact_uri(&self.quality_report_uri)?;
        self.license.validate()
    }
}

fn validate_artifact_uri(value: &str) -> Result<(), SourceContractError> {
    if !value.starts_with("artifact://") || value.len() > 512 || value.chars().any(char::is_control)
    {
        return Err(SourceContractError::InvalidArtifactUri);
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), SourceContractError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SourceContractError::InvalidDigest);
    }
    Ok(())
}

fn validate_optional_controlled(
    value: Option<&str>,
    maximum_length: usize,
) -> Result<(), SourceContractError> {
    if let Some(value) = value {
        validate_controlled(value, maximum_length)?;
    }
    Ok(())
}

fn validate_controlled(value: &str, maximum_length: usize) -> Result<(), SourceContractError> {
    if value.is_empty() || value.len() > maximum_length || value.chars().any(char::is_control) {
        return Err(SourceContractError::InvalidControlledValue);
    }
    Ok(())
}

fn validate_media_type(value: &str) -> Result<(), SourceContractError> {
    validate_controlled(value, 128)?;
    let Some((kind, subtype)) = value.split_once('/') else {
        return Err(SourceContractError::InvalidControlledValue);
    };
    if kind.is_empty()
        || subtype.is_empty()
        || value.contains(';')
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-' | b'/'
                )
        })
    {
        return Err(SourceContractError::InvalidControlledValue);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceContractError {
    InvalidEndpoint,
    InvalidHost,
    EndpointHostNotAllowed,
    InvalidMountPath,
    InvalidMountId,
    InvalidAttribution,
    InvalidControlledValue,
    InvalidSource,
    InvalidRelease,
    InvalidCoverage,
    InvalidDigest,
    InvalidArtifactUri,
    InvalidSecretReference,
    InvalidCredentialHeader,
}

impl fmt::Display for SourceContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidEndpoint => {
                "source endpoint must be an HTTPS URL without embedded credentials or a fragment"
            }
            Self::InvalidHost => "source host is invalid",
            Self::EndpointHostNotAllowed => {
                "source endpoint host must appear in its redirect allowlist"
            }
            Self::InvalidMountPath => {
                "mounted source path must be a bounded relative path without parent traversal"
            }
            Self::InvalidMountId => "source mount id must be one bounded safe identifier",
            Self::InvalidAttribution => "dataset attribution is invalid",
            Self::InvalidControlledValue => "controlled source value is invalid",
            Self::InvalidSource => "registered source invariants are invalid",
            Self::InvalidRelease => "dataset release invariants are invalid",
            Self::InvalidCoverage => "dataset coverage is invalid",
            Self::InvalidDigest => "source digest must be 64 hexadecimal SHA-256 characters",
            Self::InvalidArtifactUri => "artifact URI must use the artifact scheme",
            Self::InvalidSecretReference => "secret reference must be one bounded safe identifier",
            Self::InvalidCredentialHeader => "credential header must be a safe custom x- header",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for SourceContractError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_endpoint_rejects_credentials_and_plain_http() {
        assert!(HttpsEndpoint::parse("https://example.com/data.pbf").is_ok());
        assert!(HttpsEndpoint::parse("http://example.com/data.pbf").is_err());
        assert!(HttpsEndpoint::parse("https://user:password@example.com/data.pbf").is_err());
    }

    #[test]
    fn mounted_path_rejects_parent_traversal() {
        assert!(MountRelativePath::parse("enc/US5NY1AM.000").is_ok());
        assert!(MountRelativePath::parse("../secret").is_err());
        assert!(MountRelativePath::parse("/absolute").is_err());
    }
}
