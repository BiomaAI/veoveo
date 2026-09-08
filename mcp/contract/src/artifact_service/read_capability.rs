//! Bounded task read delegation, distinct from gateway authentication and writes.
use super::*;

/// A capability cannot enumerate artifacts or invoke a non-read operation.
/// Distinct occurrences charge their full length once; retries remain bounded
/// by the same expiry, context version, current grants, and clearance ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueArtifactReadCapabilityRequest {
    pub task_id: ArtifactTaskId,
    pub expires_at: DateTime<Utc>,
    pub max_artifact_count: NonZeroU32,
    pub max_total_bytes: NonZeroU64,
}

#[derive(Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct ArtifactReadCapabilitySecret(String);

impl ArtifactReadCapabilitySecret {
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactPlaneError> {
        let value = value.into();
        if value.len() < 32
            || value.len() > 256
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(ArtifactPlaneError::InvalidRequest(
                "invalid artifact read capability secret".into(),
            ));
        }
        Ok(Self(value))
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ArtifactReadCapabilitySecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtifactReadCapabilitySecret(<redacted>)")
    }
}

impl<'de> Deserialize<'de> for ArtifactReadCapabilitySecret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IssuedArtifactReadCapability {
    pub capability_id: ArtifactReadCapabilityId,
    pub secret: ArtifactReadCapabilitySecret,
    pub task_id: ArtifactTaskId,
    pub expires_at: DateTime<Utc>,
}

/// Local selection of an existing authority. Neither variant can mint identity.
#[derive(Clone, Copy, Debug)]
pub enum ArtifactReadAuthority<'a> {
    Caller(&'a PlaneCaller),
    Task {
        capability: &'a IssuedArtifactReadCapability,
        task_id: ArtifactTaskId,
    },
}

/// Current task credential scope. This is delegated authority, not a gateway identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReadCapabilityScope {
    pub task_id: ArtifactTaskId,
    pub principal_id: PrincipalId,
    pub principal_kind: crate::gateway::PrincipalKind,
    pub issuer: crate::gateway::TokenIssuer,
    pub subject: crate::gateway::TokenSubject,
    pub tenant: TenantId,
    pub data_labels: BTreeSet<DataLabelId>,
    pub max_total_bytes: NonZeroU64,
}
