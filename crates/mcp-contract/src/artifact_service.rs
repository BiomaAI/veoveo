//! Client contract for the shared artifact plane.
//!
//! The artifact service is the single byte-level policy-enforcement point (PEP)
//! for the platform: domain servers (media, timeseries, optimization, duckdb)
//! stop owning private buckets and instead call this service with the
//! gateway-signed identity they already received. The service owns the object
//! store and the grant ledger, and every read/write is decided by
//! [`crate::access::decide`].
//!
//! This module is transport-agnostic: it defines the [`ArtifactPlane`] trait
//! the domain servers program against, the JSON wire DTOs the HTTP service and
//! client exchange, and a typed error. The service binary and the concrete HTTP
//! client live in the `artifact-service` crate.
//!
//! Security invariants encoded here:
//!
//! - **Client never asserts tenant or owner.** [`PutArtifactRequest`] carries
//!   only presentation and client-declared sensitivity. The service stamps
//!   `tenant_id` and `owner_id` from the verified [`GatewayInternalIdentity`],
//!   so a caller cannot write into another tenant or forge ownership.
//! - **Storage key is tenant-scoped.** [`tenant_scoped_object_key`] is
//!   `t/{tenant}/artifact/{sha}`, never the bare global key, closing the
//!   content-addressed existence oracle across tenants.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::access::{AccessDecision, AccessLevel, ArtifactSha256, Grant, GroupMembership, Subject};
use crate::gateway::{DataLabelId, TenantId};
use crate::internal_auth::GatewayInternalIdentity;
use crate::storage::{ArtifactMetadata, ArtifactObject};

/// The physical, tenant-scoped object-store key for an artifact.
///
/// Unlike the retired global `crate::uri::artifact_object_key`, this partitions
/// by tenant so byte-identical content in two tenants never collides — the
/// per-tenant encryption boundary rides on this prefix.
pub fn tenant_scoped_object_key(tenant: &TenantId, sha: &ArtifactSha256) -> String {
    format!("t/{tenant}/artifact/{sha}")
}

/// What a domain server presents when acting on a principal's behalf.
///
/// The domain server has already verified the incoming gateway token; it
/// forwards the same `bearer_token` to the plane (no re-minting, no shared
/// signing secret in domain servers) and carries the parsed `identity` for
/// local reasoning. `memberships` is the `(GroupId, GroupRole)` set; it is empty
/// until groups are wired into the signed identity (P3), at which point group
/// grants begin to resolve.
#[derive(Debug, Clone)]
pub struct PlaneCaller {
    pub bearer_token: String,
    pub identity: GatewayInternalIdentity,
    pub memberships: BTreeSet<GroupMembership>,
}

impl PlaneCaller {
    /// Labels the caller carries as clearance (from the verified identity).
    pub fn clearance(&self) -> &BTreeSet<DataLabelId> {
        &self.identity.principal.data_labels
    }

    /// Tenant the caller is bound to, if any.
    pub fn tenant(&self) -> Option<&TenantId> {
        self.identity.principal.tenant.as_ref()
    }
}

/// Presentation and client-declared sensitivity for a new artifact.
///
/// Deliberately has no `tenant_id` or `owner_id`: those are stamped by the
/// service from the verified identity and can never be asserted by the client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PutArtifactRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Client-declared classification, unioned into the artifact's labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<DataLabelId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

impl PutArtifactRequest {
    /// The full label set the artifact carries: classification unioned with the
    /// explicit data labels. This is what MAC is evaluated against.
    pub fn effective_labels(&self) -> BTreeSet<DataLabelId> {
        let mut labels = self.data_labels.clone();
        if let Some(class) = &self.classification {
            labels.insert(class.clone());
        }
        labels
    }
}

/// A grant mutation request. The artifact sha travels in the request path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PutGrantRequest {
    pub subject: Subject,
    pub level: AccessLevel,
}

/// The grants recorded for one artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GrantList {
    pub grants: Vec<Grant>,
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
    /// The request was malformed (bad sha, bad uri, bad labels).
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
            Self::Denied(d) => write!(f, "access denied: {d:?}"),
            Self::Unauthenticated => f.write_str("unauthenticated"),
            Self::InvalidRequest(m) => write!(f, "invalid request: {m}"),
            Self::Conflict(m) => write!(f, "conflict: {m}"),
            Self::Transport(m) => write!(f, "transport error: {m}"),
        }
    }
}

impl std::error::Error for ArtifactPlaneError {}

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
        sha: &ArtifactSha256,
        level: AccessLevel,
    ) -> impl std::future::Future<Output = Result<ArtifactObject, ArtifactPlaneError>> + Send;

    /// Metadata only, gated at `Read`.
    fn head(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
    ) -> impl std::future::Future<Output = Result<ArtifactMetadata, ArtifactPlaneError>> + Send;

    /// Resolve a neutral `artifact://{sha}` plane URI to bytes, gated at `Read`.
    /// This is how a server feeds another server's artifact into its own tool.
    fn resolve(
        &self,
        caller: &PlaneCaller,
        uri: &str,
    ) -> impl std::future::Future<Output = Result<ArtifactObject, ArtifactPlaneError>> + Send;

    /// Add or raise a grant. Requires the caller to hold `Admin` on the artifact.
    fn grant(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
        subject: Subject,
        level: AccessLevel,
    ) -> impl std::future::Future<Output = Result<(), ArtifactPlaneError>> + Send;

    /// Remove a grant. Requires the caller to hold `Admin` on the artifact.
    fn revoke(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
        subject: &Subject,
    ) -> impl std::future::Future<Output = Result<(), ArtifactPlaneError>> + Send;

    /// List an artifact's grants. Requires `Admin` on the artifact.
    fn list_grants(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
    ) -> impl std::future::Future<Output = Result<Vec<Grant>, ArtifactPlaneError>> + Send;
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::access::decide;
    use crate::access::AccessRequest;
    use crate::gateway::{
        GatewayProfileId, GroupId, PrincipalId, PrincipalKind, ServerSlug, TokenIssuer, TokenSubject,
    };
    use crate::internal_auth::GatewayInternalIdentity;
    use crate::{JwtId, Principal};

    /// In-memory reference PEP: the canonical enforcement logic every real
    /// backend must match. Domain-server tests can reuse this shape.
    #[derive(Default)]
    struct InMemoryPlane {
        objects: Mutex<HashMap<String, Stored>>,
    }

    #[derive(Clone)]
    struct Stored {
        bytes: Vec<u8>,
        metadata: ArtifactMetadata,
        tenant: TenantId,
        labels: BTreeSet<DataLabelId>,
        grants: Vec<Grant>,
    }

    impl InMemoryPlane {
        fn authorize(
            &self,
            caller: &PlaneCaller,
            stored: &Stored,
            level: AccessLevel,
        ) -> Result<(), ArtifactPlaneError> {
            let req = AccessRequest {
                caller_id: &caller.identity.principal.id,
                caller_tenant: caller.tenant(),
                caller_labels: caller.clearance(),
                memberships: &caller.memberships,
                artifact_tenant: &stored.tenant,
                artifact_labels: &stored.labels,
                grants: &stored.grants,
                requested: level,
            };
            match decide(&req) {
                AccessDecision::Allow => Ok(()),
                other => Err(ArtifactPlaneError::Denied(other)),
            }
        }
    }

    impl ArtifactPlane for InMemoryPlane {
        async fn put(
            &self,
            caller: &PlaneCaller,
            request: PutArtifactRequest,
            bytes: Vec<u8>,
        ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
            let tenant = caller
                .tenant()
                .cloned()
                .ok_or(ArtifactPlaneError::Unauthenticated)?;
            let sha_hex = format!("{:0>64}", bytes.len()); // deterministic stub sha for tests
            let sha = ArtifactSha256::new(sha_hex)
                .map_err(|e| ArtifactPlaneError::InvalidRequest(e.to_string()))?;
            let owner = caller.identity.principal.id.clone();
            let labels = request.effective_labels();
            let metadata = ArtifactMetadata {
                sha256: sha.as_str().to_string(),
                byte_len: bytes.len() as u64,
                mime_type: request.mime_type.clone(),
                filename: request.filename.clone(),
                artifact_uri: sha.plane_uri(),
                download_url: None,
                created_at: Utc::now(),
                compliance: crate::storage::ComplianceMetadata {
                    classification: request.classification.clone(),
                    tenant_id: Some(tenant.clone()),
                    owner_id: Some(owner.clone()),
                    data_labels: request.data_labels.clone(),
                    retention_expires_at: request.retention_expires_at,
                },
                metadata: request.metadata.clone(),
            };
            // Owner gets an Admin grant automatically.
            let owner_grant = Grant {
                artifact: sha.clone(),
                subject: Subject::User(owner),
                level: AccessLevel::Admin,
                tenant: tenant.clone(),
                data_labels: labels.clone(),
                retention_expires_at: request.retention_expires_at,
            };
            let stored = Stored {
                bytes,
                metadata: metadata.clone(),
                tenant,
                labels,
                grants: vec![owner_grant],
            };
            self.objects
                .lock()
                .unwrap()
                .insert(sha.as_str().to_string(), stored);
            Ok(metadata)
        }

        async fn get(
            &self,
            caller: &PlaneCaller,
            sha: &ArtifactSha256,
            level: AccessLevel,
        ) -> Result<ArtifactObject, ArtifactPlaneError> {
            let stored = self
                .objects
                .lock()
                .unwrap()
                .get(sha.as_str())
                .cloned()
                .ok_or(ArtifactPlaneError::NotFound)?;
            self.authorize(caller, &stored, level)?;
            Ok(ArtifactObject {
                metadata: stored.metadata,
                bytes: stored.bytes,
            })
        }

        async fn head(
            &self,
            caller: &PlaneCaller,
            sha: &ArtifactSha256,
        ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
            Ok(self.get(caller, sha, AccessLevel::Read).await?.metadata)
        }

        async fn resolve(
            &self,
            caller: &PlaneCaller,
            uri: &str,
        ) -> Result<ArtifactObject, ArtifactPlaneError> {
            let sha = crate::access::parse_artifact_plane_uri(uri)
                .ok_or_else(|| ArtifactPlaneError::InvalidRequest(format!("bad plane uri: {uri}")))?;
            self.get(caller, &sha, AccessLevel::Read).await
        }

        async fn grant(
            &self,
            caller: &PlaneCaller,
            sha: &ArtifactSha256,
            subject: Subject,
            level: AccessLevel,
        ) -> Result<(), ArtifactPlaneError> {
            let mut objects = self.objects.lock().unwrap();
            let stored = objects
                .get_mut(sha.as_str())
                .ok_or(ArtifactPlaneError::NotFound)?;
            let snapshot = stored.clone();
            self.authorize(caller, &snapshot, AccessLevel::Admin)?;
            stored.grants.retain(|g| g.subject != subject);
            stored.grants.push(Grant {
                artifact: sha.clone(),
                subject,
                level,
                tenant: stored.tenant.clone(),
                data_labels: stored.labels.clone(),
                retention_expires_at: stored.metadata.compliance.retention_expires_at,
            });
            Ok(())
        }

        async fn revoke(
            &self,
            caller: &PlaneCaller,
            sha: &ArtifactSha256,
            subject: &Subject,
        ) -> Result<(), ArtifactPlaneError> {
            let mut objects = self.objects.lock().unwrap();
            let stored = objects
                .get_mut(sha.as_str())
                .ok_or(ArtifactPlaneError::NotFound)?;
            let snapshot = stored.clone();
            self.authorize(caller, &snapshot, AccessLevel::Admin)?;
            stored.grants.retain(|g| &g.subject != subject);
            Ok(())
        }

        async fn list_grants(
            &self,
            caller: &PlaneCaller,
            sha: &ArtifactSha256,
        ) -> Result<Vec<Grant>, ArtifactPlaneError> {
            let stored = self
                .objects
                .lock()
                .unwrap()
                .get(sha.as_str())
                .cloned()
                .ok_or(ArtifactPlaneError::NotFound)?;
            self.authorize(caller, &stored, AccessLevel::Admin)?;
            Ok(stored.grants)
        }
    }

    fn identity(principal_id: &str, tenant: &str, labels: &[&str]) -> GatewayInternalIdentity {
        let now = Utc::now();
        GatewayInternalIdentity {
            issuer: TokenIssuer::new("veoveo-internal").unwrap(),
            profile: GatewayProfileId::new("operator").unwrap(),
            server: ServerSlug::new("duckdb").unwrap(),
            principal: Principal {
                id: PrincipalId::new(principal_id).unwrap(),
                kind: PrincipalKind::User,
                issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
                subject: TokenSubject::new("s").unwrap(),
                tenant: Some(TenantId::new(tenant).unwrap()),
                groups: BTreeSet::new(),
                group_roles: BTreeSet::new(),
                roles: BTreeSet::new(),
                scopes: BTreeSet::new(),
                data_labels: labels
                    .iter()
                    .map(|l| DataLabelId::new(*l).unwrap())
                    .collect(),
                assurances: BTreeSet::new(),
                authenticated_at: Some(now),
            },
            jwt_id: JwtId::new(uuid::Uuid::new_v4().to_string()).unwrap(),
            issued_at: now,
            not_before: now,
            expires_at: now + chrono::TimeDelta::minutes(5),
        }
    }

    fn caller(principal_id: &str, tenant: &str, labels: &[&str]) -> PlaneCaller {
        PlaneCaller {
            bearer_token: "test-token".to_string(),
            identity: identity(principal_id, tenant, labels),
            memberships: BTreeSet::new(),
        }
    }

    #[tokio::test]
    async fn owner_can_read_back_and_others_cannot() {
        let plane = InMemoryPlane::default();
        let alice = caller("alice", "acme", &[]);
        let meta = plane
            .put(&alice, PutArtifactRequest::default(), b"hello".to_vec())
            .await
            .unwrap();
        assert_eq!(meta.compliance.tenant_id.as_ref().unwrap().as_str(), "acme");
        assert_eq!(meta.compliance.owner_id.as_ref().unwrap().as_str(), "alice");

        let sha = ArtifactSha256::new(meta.sha256.clone()).unwrap();
        // owner reads.
        let obj = plane.get(&alice, &sha, AccessLevel::Read).await.unwrap();
        assert_eq!(obj.bytes, b"hello");

        // same-tenant stranger has no grant.
        let bob = caller("bob", "acme", &[]);
        assert_eq!(
            plane.get(&bob, &sha, AccessLevel::Read).await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyNeedToKnow))
        );
    }

    #[tokio::test]
    async fn cross_tenant_read_is_denied_even_after_a_grant() {
        let plane = InMemoryPlane::default();
        let alice = caller("alice", "acme", &[]);
        let meta = plane
            .put(&alice, PutArtifactRequest::default(), b"secret".to_vec())
            .await
            .unwrap();
        let sha = ArtifactSha256::new(meta.sha256).unwrap();

        // Even if alice grants "mallory" by principal id, a different tenant loses.
        let mallory_id = Subject::User(PrincipalId::new("mallory").unwrap());
        plane
            .grant(&alice, &sha, mallory_id, AccessLevel::Read)
            .await
            .unwrap();
        let mallory = caller("mallory", "evil-corp", &[]);
        assert_eq!(
            plane.get(&mallory, &sha, AccessLevel::Read).await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyTenant))
        );
    }

    #[tokio::test]
    async fn granted_user_reads_but_cannot_write_without_the_level() {
        let plane = InMemoryPlane::default();
        let alice = caller("alice", "acme", &[]);
        let meta = plane
            .put(&alice, PutArtifactRequest::default(), b"data".to_vec())
            .await
            .unwrap();
        let sha = ArtifactSha256::new(meta.sha256).unwrap();

        plane
            .grant(
                &alice,
                &sha,
                Subject::User(PrincipalId::new("carol").unwrap()),
                AccessLevel::Read,
            )
            .await
            .unwrap();
        let carol = caller("carol", "acme", &[]);
        assert!(plane.get(&carol, &sha, AccessLevel::Read).await.is_ok());
        assert_eq!(
            plane.get(&carol, &sha, AccessLevel::Write).await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyNeedToKnow))
        );
        // carol is not admin, so she cannot re-grant.
        assert_eq!(
            plane
                .grant(
                    &carol,
                    &sha,
                    Subject::User(PrincipalId::new("dave").unwrap()),
                    AccessLevel::Read
                )
                .await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyNeedToKnow))
        );
    }

    #[tokio::test]
    async fn mac_denies_reader_without_clearance() {
        let plane = InMemoryPlane::default();
        let alice = caller("alice", "acme", &["cui"]);
        let req = PutArtifactRequest {
            classification: Some(DataLabelId::new("cui").unwrap()),
            ..Default::default()
        };
        let meta = plane.put(&alice, req, b"classified".to_vec()).await.unwrap();
        let sha = ArtifactSha256::new(meta.sha256).unwrap();

        // Grant an uncleared same-tenant user read; MAC still denies.
        plane
            .grant(
                &alice,
                &sha,
                Subject::User(PrincipalId::new("erin").unwrap()),
                AccessLevel::Read,
            )
            .await
            .unwrap();
        let erin = caller("erin", "acme", &[]); // no cui clearance
        assert_eq!(
            plane.get(&erin, &sha, AccessLevel::Read).await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyClearance))
        );
        // A cleared reader is allowed.
        let frank = caller("frank", "acme", &["cui"]);
        plane
            .grant(
                &alice,
                &sha,
                Subject::User(PrincipalId::new("frank").unwrap()),
                AccessLevel::Read,
            )
            .await
            .unwrap();
        assert!(plane.get(&frank, &sha, AccessLevel::Read).await.is_ok());
    }

    #[tokio::test]
    async fn resolve_reads_a_plane_uri() {
        let plane = InMemoryPlane::default();
        let alice = caller("alice", "acme", &[]);
        let meta = plane
            .put(&alice, PutArtifactRequest::default(), b"bytes".to_vec())
            .await
            .unwrap();
        let obj = plane.resolve(&alice, &meta.artifact_uri).await.unwrap();
        assert_eq!(obj.bytes, b"bytes");
        assert_eq!(
            plane.resolve(&alice, "artifact://nothex").await,
            Err(ArtifactPlaneError::InvalidRequest(
                "bad plane uri: artifact://nothex".to_string()
            ))
        );
    }

    #[test]
    fn put_request_wire_round_trips_without_tenant_or_owner() {
        let req = PutArtifactRequest {
            mime_type: Some("application/octet-stream".to_string()),
            filename: Some("f.bin".to_string()),
            classification: Some(DataLabelId::new("cui").unwrap()),
            data_labels: [DataLabelId::new("us_only").unwrap()].into_iter().collect(),
            retention_expires_at: None,
            metadata: json!({"k": "v"}),
        };
        let text = serde_json::to_string(&req).unwrap();
        assert!(!text.contains("tenant"), "put must not carry tenant");
        assert!(!text.contains("owner"), "put must not carry owner");
        let back: PutArtifactRequest = serde_json::from_str(&text).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn grant_and_subject_wire_round_trip() {
        let g = PutGrantRequest {
            subject: Subject::Group(GroupId::new("eng").unwrap()),
            level: AccessLevel::Write,
        };
        let text = serde_json::to_string(&g).unwrap();
        let back: PutGrantRequest = serde_json::from_str(&text).unwrap();
        assert_eq!(back, g);
    }

    #[test]
    fn tenant_scoped_key_partitions_by_tenant() {
        let sha = ArtifactSha256::new("a".repeat(64)).unwrap();
        let acme = tenant_scoped_object_key(&TenantId::new("acme").unwrap(), &sha);
        let evil = tenant_scoped_object_key(&TenantId::new("evil").unwrap(), &sha);
        assert_ne!(acme, evil);
        assert!(acme.starts_with("t/acme/artifact/"));
    }
}
