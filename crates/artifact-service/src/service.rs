//! The artifact service: the byte-level policy-enforcement point.
//!
//! It stamps tenant/owner from the verified identity, records the grant ledger,
//! decides every read/write with [`veoveo_mcp_contract::access::decide`], and
//! emits one audit line per decision carrying the reason chain.

use sha2::{Digest, Sha256};
use veoveo_mcp_contract::access::{
    AccessDecision, AccessLevel, AccessRequest, ArtifactSha256, Grant, Subject, decide,
};
use veoveo_mcp_contract::storage::{ArtifactMetadata, ArtifactObject, ComplianceMetadata};
use veoveo_mcp_contract::{
    ArtifactPlane, ArtifactPlaneError, PlaneCaller, PutArtifactRequest, parse_artifact_plane_uri,
};

use crate::ledger::{GrantLedger, StoredArtifact};
use crate::store::BlobStore;

/// Generic over the ledger and blob store so it is unit-testable without a DB.
pub struct ArtifactService<L: GrantLedger, S: BlobStore> {
    ledger: L,
    store: S,
}

impl<L: GrantLedger, S: BlobStore> ArtifactService<L, S> {
    pub fn new(ledger: L, store: S) -> Self {
        Self { ledger, store }
    }

    /// Run the access decision against a stored artifact, emitting audit
    /// evidence, and translate a denial into a typed error.
    fn authorize(
        caller: &PlaneCaller,
        stored: &StoredArtifact,
        action: &str,
        level: AccessLevel,
    ) -> Result<(), ArtifactPlaneError> {
        let request = AccessRequest {
            caller_id: &caller.identity.principal.id,
            caller_tenant: caller.tenant(),
            caller_labels: caller.clearance(),
            memberships: &caller.memberships,
            artifact_tenant: &stored.tenant,
            artifact_labels: &stored.labels,
            grants: &stored.grants,
            requested: level,
        };
        let decision = decide(&request);
        emit_audit(caller, action, &stored.metadata.sha256, level, decision);
        match decision {
            AccessDecision::Allow => Ok(()),
            other => Err(ArtifactPlaneError::Denied(other)),
        }
    }

    async fn load(&self, sha: &ArtifactSha256) -> Result<StoredArtifact, ArtifactPlaneError> {
        self.ledger
            .get_artifact(sha)
            .await
            .map_err(|e| ArtifactPlaneError::Transport(e.to_string()))?
            .ok_or(ArtifactPlaneError::NotFound)
    }
}

fn emit_audit(
    caller: &PlaneCaller,
    action: &str,
    sha: &str,
    requested: AccessLevel,
    decision: AccessDecision,
) {
    let principal = caller.identity.principal.id.as_str();
    let tenant = caller
        .tenant()
        .map(|t| t.as_str())
        .unwrap_or("<none>");
    // Single structured audit stream; reason chain is the AccessDecision variant.
    tracing::info!(
        target: "artifact_audit",
        principal,
        tenant,
        action,
        artifact_sha256 = sha,
        requested = ?requested,
        decision = ?decision,
        allowed = decision.is_allowed(),
        "artifact access decision"
    );
}

fn compute_sha(bytes: &[u8]) -> ArtifactSha256 {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    // hex of a sha256 is always valid; unwrap is safe.
    ArtifactSha256::new(hex::encode(digest)).expect("sha256 hex is valid")
}

impl<L: GrantLedger, S: BlobStore> ArtifactPlane for ArtifactService<L, S> {
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
        let owner = caller.identity.principal.id.clone();
        let sha = compute_sha(&bytes);
        let labels = request.effective_labels();

        // Store bytes first (encrypted, tenant-scoped); ledger records truth.
        self.store
            .put(&tenant, &sha, bytes.clone())
            .await
            .map_err(|e| ArtifactPlaneError::Transport(e.to_string()))?;

        let metadata = ArtifactMetadata {
            sha256: sha.as_str().to_string(),
            byte_len: bytes.len() as u64,
            mime_type: request.mime_type.clone(),
            filename: request.filename.clone(),
            artifact_uri: sha.plane_uri(),
            download_url: None,
            created_at: chrono::Utc::now(),
            compliance: ComplianceMetadata {
                classification: request.classification.clone(),
                tenant_id: Some(tenant.clone()),
                owner_id: Some(owner.clone()),
                data_labels: request.data_labels.clone(),
                retention_expires_at: request.retention_expires_at,
            },
            metadata: request.metadata.clone(),
        };

        let owner_grant = Grant {
            artifact: sha.clone(),
            subject: Subject::User(owner),
            level: AccessLevel::Admin,
            tenant: tenant.clone(),
            data_labels: labels.clone(),
            retention_expires_at: request.retention_expires_at,
        };

        self.ledger
            .insert_artifact(StoredArtifact {
                metadata: metadata.clone(),
                tenant,
                labels,
                grants: vec![owner_grant],
            })
            .await
            .map_err(|e| ArtifactPlaneError::Transport(e.to_string()))?;

        emit_audit(
            caller,
            "put",
            &metadata.sha256,
            AccessLevel::Admin,
            AccessDecision::Allow,
        );
        Ok(metadata)
    }

    async fn get(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
        level: AccessLevel,
    ) -> Result<ArtifactObject, ArtifactPlaneError> {
        let stored = self.load(sha).await?;
        Self::authorize(caller, &stored, "get", level)?;
        let bytes = self
            .store
            .get(&stored.tenant, sha)
            .await
            .map_err(|e| ArtifactPlaneError::Transport(e.to_string()))?;
        Ok(ArtifactObject {
            metadata: stored.metadata,
            bytes,
        })
    }

    async fn head(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let stored = self.load(sha).await?;
        Self::authorize(caller, &stored, "head", AccessLevel::Read)?;
        Ok(stored.metadata)
    }

    async fn resolve(
        &self,
        caller: &PlaneCaller,
        uri: &str,
    ) -> Result<ArtifactObject, ArtifactPlaneError> {
        let sha = parse_artifact_plane_uri(uri)
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
        let stored = self.load(sha).await?;
        Self::authorize(caller, &stored, "grant", AccessLevel::Admin)?;
        let grant = Grant {
            artifact: sha.clone(),
            subject,
            level,
            tenant: stored.tenant.clone(),
            data_labels: stored.labels.clone(),
            retention_expires_at: stored.metadata.compliance.retention_expires_at,
        };
        self.ledger
            .upsert_grant(grant)
            .await
            .map_err(|e| ArtifactPlaneError::Transport(e.to_string()))
    }

    async fn revoke(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
        subject: &Subject,
    ) -> Result<(), ArtifactPlaneError> {
        let stored = self.load(sha).await?;
        Self::authorize(caller, &stored, "revoke", AccessLevel::Admin)?;
        self.ledger
            .remove_grant(sha, subject)
            .await
            .map_err(|e| ArtifactPlaneError::Transport(e.to_string()))
    }

    async fn list_grants(
        &self,
        caller: &PlaneCaller,
        sha: &ArtifactSha256,
    ) -> Result<Vec<Grant>, ArtifactPlaneError> {
        let stored = self.load(sha).await?;
        Self::authorize(caller, &stored, "list_grants", AccessLevel::Admin)?;
        Ok(stored.grants)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::Utc;
    use veoveo_mcp_contract::gateway::{
        DataLabelId, GatewayProfileId, PrincipalId, PrincipalKind, ServerSlug, TokenIssuer,
        TokenSubject,
    };
    use veoveo_mcp_contract::internal_auth::GatewayInternalIdentity;
    use veoveo_mcp_contract::gateway::TenantId;
    use veoveo_mcp_contract::{JwtId, Principal};

    use super::*;
    use crate::ledger::testing::InMemoryLedger;
    use crate::store::testing::InMemoryBlobStore;

    fn caller(principal: &str, tenant: &str, labels: &[&str]) -> PlaneCaller {
        let now = Utc::now();
        PlaneCaller {
            bearer_token: "t".to_string(),
            identity: GatewayInternalIdentity {
                issuer: TokenIssuer::new("veoveo-internal").unwrap(),
                profile: GatewayProfileId::new("operator").unwrap(),
                server: ServerSlug::new("duckdb").unwrap(),
                principal: Principal {
                    id: PrincipalId::new(principal).unwrap(),
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
            },
            memberships: BTreeSet::new(),
        }
    }

    fn service() -> ArtifactService<InMemoryLedger, InMemoryBlobStore> {
        ArtifactService::new(InMemoryLedger::default(), InMemoryBlobStore::default())
    }

    #[tokio::test]
    async fn put_then_owner_get_roundtrips_bytes() {
        let svc = service();
        let alice = caller("alice", "acme", &[]);
        let meta = svc
            .put(&alice, PutArtifactRequest::default(), b"payload".to_vec())
            .await
            .unwrap();
        let sha = ArtifactSha256::new(meta.sha256.clone()).unwrap();
        assert_eq!(meta.compliance.tenant_id.unwrap().as_str(), "acme");
        let obj = svc.get(&alice, &sha, AccessLevel::Read).await.unwrap();
        assert_eq!(obj.bytes, b"payload");
    }

    #[tokio::test]
    async fn cross_tenant_get_is_denied() {
        let svc = service();
        let alice = caller("alice", "acme", &[]);
        let meta = svc
            .put(&alice, PutArtifactRequest::default(), b"x".to_vec())
            .await
            .unwrap();
        let sha = ArtifactSha256::new(meta.sha256).unwrap();
        // grant mallory by id, but she is in another tenant.
        svc.grant(
            &alice,
            &sha,
            Subject::User(PrincipalId::new("mallory").unwrap()),
            AccessLevel::Read,
        )
        .await
        .unwrap();
        let mallory = caller("mallory", "evil", &[]);
        assert_eq!(
            svc.get(&mallory, &sha, AccessLevel::Read).await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyTenant))
        );
    }

    #[tokio::test]
    async fn mac_denies_uncleared_reader() {
        let svc = service();
        let alice = caller("alice", "acme", &["cui"]);
        let req = PutArtifactRequest {
            classification: Some(DataLabelId::new("cui").unwrap()),
            ..Default::default()
        };
        let meta = svc.put(&alice, req, b"secret".to_vec()).await.unwrap();
        let sha = ArtifactSha256::new(meta.sha256).unwrap();
        svc.grant(
            &alice,
            &sha,
            Subject::User(PrincipalId::new("erin").unwrap()),
            AccessLevel::Read,
        )
        .await
        .unwrap();
        let erin = caller("erin", "acme", &[]);
        assert_eq!(
            svc.get(&erin, &sha, AccessLevel::Read).await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyClearance))
        );
    }

    #[tokio::test]
    async fn identical_bytes_dedup_to_same_sha() {
        let svc = service();
        let alice = caller("alice", "acme", &[]);
        let a = svc
            .put(&alice, PutArtifactRequest::default(), b"dup".to_vec())
            .await
            .unwrap();
        let b = svc
            .put(&alice, PutArtifactRequest::default(), b"dup".to_vec())
            .await
            .unwrap();
        assert_eq!(a.sha256, b.sha256);
    }

    #[tokio::test]
    async fn non_admin_cannot_grant() {
        let svc = service();
        let alice = caller("alice", "acme", &[]);
        let meta = svc
            .put(&alice, PutArtifactRequest::default(), b"z".to_vec())
            .await
            .unwrap();
        let sha = ArtifactSha256::new(meta.sha256).unwrap();
        svc.grant(
            &alice,
            &sha,
            Subject::User(PrincipalId::new("carol").unwrap()),
            AccessLevel::Read,
        )
        .await
        .unwrap();
        let carol = caller("carol", "acme", &[]);
        assert_eq!(
            svc.grant(
                &carol,
                &sha,
                Subject::User(PrincipalId::new("dave").unwrap()),
                AccessLevel::Read
            )
            .await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyNeedToKnow))
        );
    }
}
