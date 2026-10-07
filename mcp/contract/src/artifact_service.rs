//! Verified Artifact authority and asynchronous plane adapter.
//! Pure requests, results, limits and admission belong to the Artifact contract.
use crate::{
    access::{AccessDecision, GroupMembership},
    internal_auth::GatewayInternalIdentity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, num::NonZeroU64};
use veoveo_artifact_contract::*;
use veoveo_types::{AccessLevel, AccessSubject, DataLabelId, PrincipalId, TenantId};
impl From<ArtifactLedgerIdError> for ArtifactPlaneError {
    fn from(error: ArtifactLedgerIdError) -> Self {
        Self::InvalidRequest(error.to_string())
    }
}
impl From<ArtifactWireError> for ArtifactPlaneError {
    fn from(error: ArtifactWireError) -> Self {
        Self::InvalidRequest(error.detail().to_owned())
    }
}
/// What a domain server presents when acting on a principal's behalf.
///
/// The domain server has already verified the incoming gateway token; it
/// forwards the same `bearer_token` to the plane (no re-minting, no shared
/// signing secret in domain servers) and carries the parsed `identity` for
/// local reasoning. `memberships` is the `(GroupId, GroupRole)` set derived from
/// the signed principal and used to resolve group grants.
#[derive(Clone)]
pub struct PlaneCaller {
    pub bearer_token: String,
    pub identity: GatewayInternalIdentity,
    pub memberships: BTreeSet<GroupMembership>,
}

impl fmt::Debug for PlaneCaller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlaneCaller")
            .field("bearer_token", &"<redacted>")
            .field("identity", &self.identity)
            .field("memberships", &self.memberships)
            .finish()
    }
}

impl PlaneCaller {
    /// Labels the caller carries as clearance (from the verified identity).
    pub fn clearance(&self) -> &BTreeSet<DataLabelId> {
        &self.identity.actor.data_labels
    }

    /// Tenant the caller is bound to, if any.
    pub fn tenant(&self) -> Option<&TenantId> {
        self.identity.actor.tenant.as_ref()
    }
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
/// Typed failure surface for the plane. `Denied` carries the [`AccessDecision`]
/// so audit evidence keeps the reason chain (tenant / clearance / need-to-know).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactPlaneError {
    /// No such artifact (or the caller may not learn that it exists).
    NotFound,
    /// Access denied, with the deciding reason for audit.
    Denied(AccessDecision),
    /// The caller presented no usable identity.
    Unauthenticated,
    /// The request was malformed (bad id, URI, labels, or limits).
    InvalidRequest(String),
    /// A conflicting state prevented the mutation.
    Conflict(String),
    /// Transport or backend failure.
    Transport(String),
}

impl std::fmt::Display for ArtifactPlaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("artifact not found"),
            Self::Denied(d) => write!(f, "access denied: {}", denial_reason(*d)),
            Self::Unauthenticated => f.write_str("unauthenticated"),
            Self::InvalidRequest(m) => write!(f, "invalid request: {m}"),
            Self::Conflict(m) => write!(f, "conflict: {m}"),
            Self::Transport(m) => write!(f, "transport error: {m}"),
        }
    }
}

impl std::error::Error for ArtifactPlaneError {}

fn denial_reason(decision: AccessDecision) -> &'static str {
    match decision {
        AccessDecision::Allow => "allowed",
        AccessDecision::DenyTenant => "the artifact belongs to another tenant",
        AccessDecision::DenyClearance => "the caller is not cleared for the artifact's data labels",
        AccessDecision::DenyNeedToKnow => "no grant gives the caller access",
    }
}

/// The interface every domain server programs against to reach the plane.
///
/// Uses native `async fn` in traits (edition 2024). Callers are generic over
/// the concrete plane implementation, so no dynamic dispatch or `async-trait`
/// dependency is needed; tests substitute an in-memory reference PEP.
pub trait ArtifactPlane {
    /// Store `bytes`, stamping tenant/owner from the caller's identity and
    /// recording an owner `Admin` grant. Returns the canonical metadata.
    fn put(
        &self,
        caller: &PlaneCaller,
        request: PutArtifactRequest,
        bytes: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<ArtifactMetadata, ArtifactPlaneError>> + Send;

    /// Fetch bytes + metadata if the caller has at least `level`.
    fn get(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        level: AccessLevel,
    ) -> impl std::future::Future<Output = Result<ArtifactObject, ArtifactPlaneError>> + Send;

    /// Metadata only, gated at `Read`.
    fn head(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
    ) -> impl std::future::Future<Output = Result<ArtifactMetadata, ArtifactPlaneError>> + Send;

    /// Metadata and recorded access state from one repository snapshot, gated at
    /// Read. This contains no transfer locations or bearer share links.
    fn metadata_snapshot(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
    ) -> impl std::future::Future<
        Output = Result<veoveo_artifact_contract::ArtifactMetadataSnapshot, ArtifactPlaneError>,
    > + Send;

    /// Discover metadata for artifacts on which the caller has at least read
    /// access. Results are ordered newest first and keyset-paginated.
    fn list(
        &self,
        caller: &PlaneCaller,
        request: ListArtifactsRequest,
    ) -> impl std::future::Future<Output = Result<ArtifactPage, ArtifactPlaneError>> + Send;

    /// Resolve a neutral `artifact://{id}` plane URI to bytes, gated at `Read`.
    /// This is how a server feeds another server's artifact into its own tool.
    fn resolve(
        &self,
        caller: &PlaneCaller,
        uri: &veoveo_artifact_contract::ArtifactUri,
    ) -> impl std::future::Future<Output = Result<ArtifactObject, ArtifactPlaneError>> + Send;

    /// Add or raise a grant. Requires the caller to hold `Admin` on the artifact.
    fn grant(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        subject: AccessSubject,
        level: AccessLevel,
    ) -> impl std::future::Future<Output = Result<(), ArtifactPlaneError>> + Send;

    /// Remove a grant. Requires the caller to hold `Admin` on the artifact.
    fn revoke(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        subject: &AccessSubject,
    ) -> impl std::future::Future<Output = Result<(), ArtifactPlaneError>> + Send;

    /// List an artifact's grants. Requires `Admin` on the artifact.
    fn list_grants(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
    ) -> impl std::future::Future<Output = Result<Vec<Grant>, ArtifactPlaneError>> + Send;

    /// Change whether an artifact may be published. Requires `Admin`.
    fn set_release_state(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        release_state: ArtifactReleaseState,
    ) -> impl std::future::Future<Output = Result<ArtifactMetadata, ArtifactPlaneError>> + Send;

    /// Create a read-only anyone-with-link bearer. Requires `Admin` and an
    /// explicitly releasable artifact.
    fn create_share_link(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        request: CreateArtifactShareLinkRequest,
    ) -> impl std::future::Future<Output = Result<ArtifactShareLink, ArtifactPlaneError>> + Send;

    /// Revoke a public share link. Requires `Admin` on its artifact.
    fn revoke_share_link(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        link_id: &ArtifactShareLinkId,
    ) -> impl std::future::Future<Output = Result<(), ArtifactPlaneError>> + Send;

    fn create_access_request(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        request: CreateArtifactAccessRequest,
    ) -> impl std::future::Future<Output = Result<ArtifactAccessRequest, ArtifactPlaneError>> + Send;

    fn list_access_requests(
        &self,
        caller: &PlaneCaller,
        request: ListArtifactAccessRequests,
    ) -> impl std::future::Future<Output = Result<ArtifactAccessRequestPage, ArtifactPlaneError>> + Send;

    fn decide_access_request(
        &self,
        caller: &PlaneCaller,
        request_id: &ArtifactAccessRequestId,
        decision: DecideArtifactAccessRequest,
    ) -> impl std::future::Future<Output = Result<ArtifactAccessRequest, ArtifactPlaneError>> + Send;

    fn cancel_access_request(
        &self,
        caller: &PlaneCaller,
        request_id: &ArtifactAccessRequestId,
    ) -> impl std::future::Future<Output = Result<ArtifactAccessRequest, ArtifactPlaneError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_admission_becomes_one_safe_adapter_prefix() {
        let owner = ArtifactWriteCapabilitySecret::new("short").unwrap_err();
        let detail = owner.detail();
        let adapter = ArtifactPlaneError::from(owner);
        assert!(matches!(&adapter, ArtifactPlaneError::InvalidRequest(value) if value == detail));
        assert_eq!(adapter.to_string(), format!("invalid request: {detail}"));
        assert!(!adapter.to_string().contains("short"));
    }
}
