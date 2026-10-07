//! Artifact access values; policy evaluation stays with the service.
use crate::ArtifactId;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, num::NonZeroU64};
use veoveo_types::{AccessLevel, AccessSubject, DataLabelId, TenantId};

/// One entry in an artifact's access control list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grant {
    pub artifact: ArtifactId,
    pub subject: AccessSubject,
    pub level: AccessLevel,
    /// Tenant the artifact (and therefore this grant) lives in. Isolation is a
    /// hard partition: a grant never bridges tenants.
    pub tenant: TenantId,
    /// Labels the artifact carries. MAC is checked against these independently
    /// of the grant; no grant can widen clearance.
    #[serde(default)]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<veoveo_types::ChronoUtcTimestampSchema>")]
    pub retention_expires_at: Option<DateTime<Utc>>,
}

impl veoveo_types::AccessGrant for Grant {
    fn subject(&self) -> &AccessSubject {
        &self.subject
    }
    fn level(&self) -> AccessLevel {
        self.level
    }
    fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.retention_expires_at
    }
}

/// Identity of an expiring Artifact share, distinct from an occurrence.
/// ```compile_fail
/// use veoveo_artifact_contract::{ArtifactId, ArtifactShareLinkId};
/// fn revoke(_: ArtifactShareLinkId) {}
/// revoke(ArtifactId::new());
/// ```
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct ArtifactShareLinkId(uuid::Uuid);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("artifact share link id must be an RFC UUIDv7")]
pub struct ArtifactShareLinkIdError;

impl ArtifactShareLinkId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ArtifactShareLinkIdError> {
        let id = uuid::Uuid::parse_str(value.as_ref()).map_err(|_| ArtifactShareLinkIdError)?;
        if id.get_version_num() != 7 || id.get_variant() != uuid::Variant::RFC4122 {
            return Err(ArtifactShareLinkIdError);
        }
        Ok(Self(id))
    }
    pub const fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl Default for ArtifactShareLinkId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for ArtifactShareLinkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl TryFrom<String> for ArtifactShareLinkId {
    type Error = ArtifactShareLinkIdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<ArtifactShareLinkId> for String {
    fn from(value: ArtifactShareLinkId) -> Self {
        value.to_string()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactShareLink {
    pub link_id: ArtifactShareLinkId,
    pub artifact_id: ArtifactId,
    pub url: String,
    #[schemars(with = "veoveo_types::ChronoUtcTimestampSchema")]
    pub expires_at: DateTime<Utc>,
    pub max_downloads: Option<NonZeroU64>,
}

impl fmt::Debug for ArtifactShareLink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ArtifactShareLink")
            .field("link_id", &self.link_id)
            .field("artifact_id", &self.artifact_id)
            .field("url", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("max_downloads", &self.max_downloads)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn share_identity_keeps_its_wire_shape_and_rejects_wrong_uuid_profiles() {
        let id = ArtifactShareLinkId::new();
        let encoded = serde_json::to_value(id).unwrap();
        assert_eq!(encoded, id.to_string());
        assert_eq!(
            serde_json::from_value::<ArtifactShareLinkId>(encoded).unwrap(),
            id
        );
        for invalid in [
            "secret-invalid-id",
            "01983da0-0000-4000-8000-000000000001",
            "01983da0-0000-7000-0000-000000000001",
        ] {
            let error = ArtifactShareLinkId::parse(invalid).unwrap_err();
            assert!(!error.to_string().contains(invalid));
            assert!(serde_json::from_value::<ArtifactShareLinkId>(invalid.into()).is_err());
        }
    }

    #[test]
    fn sharing_values_preserve_serialization_and_secret_redaction() {
        let grant = Grant {
            artifact: ArtifactId::new(),
            subject: AccessSubject::Principal("operator".parse().unwrap()),
            level: AccessLevel::Read,
            tenant: "example".parse().unwrap(),
            data_labels: BTreeSet::new(),
            retention_expires_at: None,
        };
        let encoded = serde_json::to_value(&grant).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({"artifact": grant.artifact,
            "subject": {"kind": "principal", "id": "operator"}, "level": "read",
            "tenant": "example", "dataLabels": []})
        );
        assert_eq!(serde_json::from_value::<Grant>(encoded).unwrap(), grant);
        let share = ArtifactShareLink {
            link_id: ArtifactShareLinkId::new(),
            artifact_id: grant.artifact,
            url: "https://example.test/s/private-bearer".into(),
            expires_at: "2026-10-01T00:00:00Z".parse().unwrap(),
            max_downloads: NonZeroU64::new(1),
        };
        assert!(!format!("{share:?}").contains("private-bearer"));
        assert_eq!(
            serde_json::from_value::<ArtifactShareLink>(serde_json::to_value(&share).unwrap())
                .unwrap(),
            share
        );
    }
}

/// Flat subject kind used by Artifact grant projections.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum ArtifactGrantSubjectKind {
    #[vocabulary(rename = "principal")]
    Principal,
    #[vocabulary(rename = "group")]
    Group,
}
