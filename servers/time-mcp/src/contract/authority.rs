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
pub struct AuthorityBinding(veoveo_types::Checked<BindingWire>);

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct BindingWire {
    tzdb_release_id: AuthorityReleaseId,
    leap_seconds_release_id: AuthorityReleaseId,
}

impl AuthorityBinding {
    pub fn new(
        tzdb_release_id: AuthorityReleaseId,
        leap_seconds_release_id: AuthorityReleaseId,
    ) -> Result<Self, TimeAuthorityError> {
        veoveo_types::Checked::new(BindingWire {
            tzdb_release_id,
            leap_seconds_release_id,
        })
        .map(Self)
    }
    pub fn tzdb_release_id(&self) -> &AuthorityReleaseId {
        &self.0.tzdb_release_id
    }
    pub fn leap_seconds_release_id(&self) -> &AuthorityReleaseId {
        &self.0.leap_seconds_release_id
    }
}

impl veoveo_types::Check for BindingWire {
    type Error = TimeAuthorityError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.tzdb_release_id == self.leap_seconds_release_id {
            return Err(TimeAuthorityError::DuplicateRelease);
        }

        Ok(())
    }
}
impl TryFrom<BindingWire> for AuthorityBinding {
    type Error = TimeAuthorityError;
    fn try_from(value: BindingWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<AuthorityBinding> for BindingWire {
    fn from(value: AuthorityBinding) -> Self {
        value.0.into_inner()
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
pub struct EffectiveTimeAuthority(veoveo_types::Checked<EffectiveWire>);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
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
        veoveo_types::Checked::new(EffectiveWire { tzdb, leap_seconds }).map(Self)
    }
    pub fn tzdb(&self) -> &TimeAuthorityReference {
        &self.0.tzdb
    }
    pub fn leap_seconds(&self) -> &TimeAuthorityReference {
        &self.0.leap_seconds
    }
    pub fn binding(&self) -> AuthorityBinding {
        AuthorityBinding::new(
            self.0.tzdb.release_id().clone(),
            self.0.leap_seconds.release_id().clone(),
        )
        .unwrap_or_else(|_| unreachable!("admitted authority releases are distinct"))
    }
}

impl veoveo_types::Check for EffectiveWire {
    type Error = TimeAuthorityError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.tzdb.dataset_kind() != AuthorityDatasetKind::Tzdb
            || self.leap_seconds.dataset_kind() != AuthorityDatasetKind::LeapSeconds
        {
            return Err(TimeAuthorityError::DatasetKind);
        }
        if self.tzdb.release_id() == self.leap_seconds.release_id() {
            return Err(TimeAuthorityError::DuplicateRelease);
        }

        Ok(())
    }
}
impl TryFrom<EffectiveWire> for EffectiveTimeAuthority {
    type Error = TimeAuthorityError;
    fn try_from(value: EffectiveWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<EffectiveTimeAuthority> for EffectiveWire {
    fn from(value: EffectiveTimeAuthority) -> Self {
        value.0.into_inner()
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
