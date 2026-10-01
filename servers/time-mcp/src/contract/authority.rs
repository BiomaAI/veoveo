use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::Sha256Digest;

use super::{
    AuthorityDatasetKind, AuthorityReleaseId, TimeAcquisitionId, TimeAuthorityReleaseUri,
    TimeSourceId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeAuthorityError {
    ReleaseIdentity,
    SourceLocation,
    DatasetKind,
    DuplicateRelease,
    BlankVersionLabel,
}

impl fmt::Display for TimeAuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SourceLocation => "time authority source and resource location must agree",
            Self::ReleaseIdentity => "time authority release URI and identity must agree",
            Self::DatasetKind => "effective time authority requires TZDB and leap-second references in their declared roles",
            Self::DuplicateRelease => "TZDB and leap-second authority must have distinct release identities",
            Self::BlankVersionLabel => "time authority version label must not be blank",
        })
    }
}

impl Error for TimeAuthorityError {}

/// Distinct release identities used to interpret an instant.
/// ```compile_fail
/// use veoveo_time_mcp::{AuthorityBinding, AuthorityReleaseId};
/// fn cannot_replace_one_family(binding: &mut AuthorityBinding, id: AuthorityReleaseId) {
///     binding.tzdb_release_id = id;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "BindingWire", into = "BindingWire")]
pub struct AuthorityBinding {
    tzdb_release_id: AuthorityReleaseId,
    leap_seconds_release_id: AuthorityReleaseId,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct BindingWire {
    tzdb_release_id: AuthorityReleaseId,
    leap_seconds_release_id: AuthorityReleaseId,
}

impl AuthorityBinding {
    pub fn new(
        tzdb_release_id: AuthorityReleaseId,
        leap_seconds_release_id: AuthorityReleaseId,
    ) -> Result<Self, TimeAuthorityError> {
        if tzdb_release_id == leap_seconds_release_id {
            return Err(TimeAuthorityError::DuplicateRelease);
        }
        Ok(Self {
            tzdb_release_id,
            leap_seconds_release_id,
        })
    }
    pub fn tzdb_release_id(&self) -> &AuthorityReleaseId {
        &self.tzdb_release_id
    }
    pub fn leap_seconds_release_id(&self) -> &AuthorityReleaseId {
        &self.leap_seconds_release_id
    }
}

impl TryFrom<BindingWire> for AuthorityBinding {
    type Error = TimeAuthorityError;
    fn try_from(wire: BindingWire) -> Result<Self, Self::Error> {
        Self::new(wire.tzdb_release_id, wire.leap_seconds_release_id)
    }
}
impl From<AuthorityBinding> for BindingWire {
    fn from(value: AuthorityBinding) -> Self {
        Self {
            tzdb_release_id: value.tzdb_release_id,
            leap_seconds_release_id: value.leap_seconds_release_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimeAuthoritySource {
    Bootstrap,
    Acquisition {
        source_id: TimeSourceId,
        acquisition_id: TimeAcquisitionId,
    },
}

/// A release reference whose wire identity is derived from its URI.
/// ```compile_fail
/// use veoveo_time_mcp::{TimeAuthorityReference, AuthorityReleaseId};
/// fn cannot_relabel(reference: &mut TimeAuthorityReference, id: AuthorityReleaseId) {
///     reference.release_id = id;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ReferenceWire", into = "ReferenceWire")]
pub struct TimeAuthorityReference {
    release_uri: TimeAuthorityReleaseUri,
    dataset_kind: AuthorityDatasetKind,
    source: TimeAuthoritySource,
    source_digest: Sha256Digest,
    version_label: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReferenceWire {
    release_uri: TimeAuthorityReleaseUri,
    release_id: AuthorityReleaseId,
    dataset_kind: AuthorityDatasetKind,
    source: TimeAuthoritySource,
    source_digest: Sha256Digest,
    /// A label containing at least one non-whitespace character.
    #[schemars(length(min = 1))]
    version_label: String,
}

impl TimeAuthorityReference {
    pub fn new(
        release_uri: TimeAuthorityReleaseUri,
        dataset_kind: AuthorityDatasetKind,
        source: TimeAuthoritySource,
        source_digest: Sha256Digest,
        version_label: String,
    ) -> Result<Self, TimeAuthorityError> {
        if release_uri.is_bootstrap() != matches!(source, TimeAuthoritySource::Bootstrap) {
            return Err(TimeAuthorityError::SourceLocation);
        }
        if version_label.trim().is_empty() {
            return Err(TimeAuthorityError::BlankVersionLabel);
        }
        Ok(Self {
            release_uri,
            dataset_kind,
            source,
            source_digest,
            version_label,
        })
    }
    pub fn release_uri(&self) -> &TimeAuthorityReleaseUri {
        &self.release_uri
    }
    pub fn release_id(&self) -> &AuthorityReleaseId {
        self.release_uri.release_id()
    }
    pub fn dataset_kind(&self) -> AuthorityDatasetKind {
        self.dataset_kind
    }
    pub fn source(&self) -> &TimeAuthoritySource {
        &self.source
    }
    pub fn source_digest(&self) -> &Sha256Digest {
        &self.source_digest
    }
    pub fn version_label(&self) -> &str {
        &self.version_label
    }
}

impl TryFrom<ReferenceWire> for TimeAuthorityReference {
    type Error = TimeAuthorityError;
    fn try_from(wire: ReferenceWire) -> Result<Self, Self::Error> {
        if wire.release_uri.release_id() != &wire.release_id {
            return Err(TimeAuthorityError::ReleaseIdentity);
        }
        Self::new(
            wire.release_uri,
            wire.dataset_kind,
            wire.source,
            wire.source_digest,
            wire.version_label,
        )
    }
}
impl From<TimeAuthorityReference> for ReferenceWire {
    fn from(value: TimeAuthorityReference) -> Self {
        Self {
            release_id: value.release_id().clone(),
            release_uri: value.release_uri,
            dataset_kind: value.dataset_kind,
            source: value.source,
            source_digest: value.source_digest,
            version_label: value.version_label,
        }
    }
}

/// A checked pair with one distinct release per dataset family.
/// ```compile_fail
/// use veoveo_time_mcp::{EffectiveTimeAuthority, TimeAuthorityReference};
/// fn cannot_swap_family(pair: &mut EffectiveTimeAuthority, reference: TimeAuthorityReference) {
///     pair.tzdb = reference;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "EffectiveWire", into = "EffectiveWire")]
pub struct EffectiveTimeAuthority {
    tzdb: TimeAuthorityReference,
    leap_seconds: TimeAuthorityReference,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectiveWire {
    tzdb: TimeAuthorityReference,
    leap_seconds: TimeAuthorityReference,
}

impl EffectiveTimeAuthority {
    pub fn new(
        tzdb: TimeAuthorityReference,
        leap_seconds: TimeAuthorityReference,
    ) -> Result<Self, TimeAuthorityError> {
        if tzdb.dataset_kind() != AuthorityDatasetKind::Tzdb
            || leap_seconds.dataset_kind() != AuthorityDatasetKind::LeapSeconds
        {
            return Err(TimeAuthorityError::DatasetKind);
        }
        if tzdb.release_id() == leap_seconds.release_id() {
            return Err(TimeAuthorityError::DuplicateRelease);
        }
        Ok(Self { tzdb, leap_seconds })
    }
    pub fn tzdb(&self) -> &TimeAuthorityReference {
        &self.tzdb
    }
    pub fn leap_seconds(&self) -> &TimeAuthorityReference {
        &self.leap_seconds
    }
    pub fn binding(&self) -> AuthorityBinding {
        AuthorityBinding {
            tzdb_release_id: self.tzdb.release_id().clone(),
            leap_seconds_release_id: self.leap_seconds.release_id().clone(),
        }
    }
}

impl TryFrom<EffectiveWire> for EffectiveTimeAuthority {
    type Error = TimeAuthorityError;
    fn try_from(wire: EffectiveWire) -> Result<Self, Self::Error> {
        Self::new(wire.tzdb, wire.leap_seconds)
    }
}
impl From<EffectiveTimeAuthority> for EffectiveWire {
    fn from(value: EffectiveTimeAuthority) -> Self {
        Self {
            tzdb: value.tzdb,
            leap_seconds: value.leap_seconds,
        }
    }
}

impl JsonSchema for EffectiveTimeAuthority {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EffectiveTimeAuthority".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let reference = generator.subschema_for::<TimeAuthorityReference>();
        schemars::json_schema!({
            "type":"object", "additionalProperties":false, "required":["tzdb","leap_seconds"],
            "properties": {
                "tzdb": {"allOf":[reference.clone(), {"properties":{"dataset_kind":{"const":AuthorityDatasetKind::Tzdb}}}]},
                "leap_seconds": {"allOf":[reference, {"properties":{"dataset_kind":{"const":AuthorityDatasetKind::LeapSeconds}}}]}
            }
        })
    }
}
