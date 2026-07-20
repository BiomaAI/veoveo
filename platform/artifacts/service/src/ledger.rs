//! Durable artifact occurrence, authorization, capability, sharing, and audit state.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use veoveo_mcp_contract::access::{AccessSubject, ArtifactId, Grant};
use veoveo_mcp_contract::gateway::{
    DataLabelId, GatewayProfileId, GroupId, PrincipalId, PrincipalKind, ServerSlug, TenantId,
    TokenIssuer, TokenSubject,
};
use veoveo_mcp_contract::storage::{ArtifactMetadata, ArtifactReleaseState};
use veoveo_mcp_contract::{ArtifactShareLinkId, ArtifactWriteCapabilityId, InvocationAuthority};

/// Full verified identity needed to create stable platform records and audit actors.
#[derive(Debug, Clone)]
pub struct RepositoryActor {
    pub tenant: TenantId,
    pub principal: PrincipalId,
    pub kind: PrincipalKind,
    pub issuer: TokenIssuer,
    pub subject: TokenSubject,
}

/// One public occurrence plus its internal tenant-local blob identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlobSha256(String);

impl BlobSha256 {
    pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value))
        } else {
            Err("blob sha256 must be 64 lowercase hex characters")
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct StoredArtifact {
    pub metadata: ArtifactMetadata,
    pub tenant: TenantId,
    pub labels: BTreeSet<DataLabelId>,
    pub grants: Vec<Grant>,
    pub authority: InvocationAuthority,
    pub(crate) blob_sha256: BlobSha256,
    pub object_key: String,
}

#[derive(Debug, Clone)]
pub struct NewArtifact {
    pub actor: RepositoryActor,
    pub stored: StoredArtifact,
}

#[derive(Debug, Clone)]
pub struct ArtifactListQuery {
    pub actor: RepositoryActor,
    pub groups: BTreeSet<GroupId>,
    pub cursor: Option<ArtifactId>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct WriteCapabilityDraft {
    pub capability_id: ArtifactWriteCapabilityId,
    pub actor: RepositoryActor,
    pub authority: InvocationAuthority,
    pub profile: GatewayProfileId,
    pub server: ServerSlug,
    pub task_id: String,
    pub token_hash: String,
    pub labels: BTreeSet<DataLabelId>,
    pub max_artifact_count: u32,
    pub max_total_bytes: u64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WriteCapabilityReservation {
    pub capability_id: ArtifactWriteCapabilityId,
    pub token_hash: String,
    pub task_id: String,
    pub idempotency_key: String,
    pub request_hash: String,
    pub byte_len: u64,
    pub requested_labels: BTreeSet<DataLabelId>,
    pub proposed_artifact_id: ArtifactId,
}

#[derive(Debug, Clone)]
pub struct RedeemedWriteCapability {
    pub redemption_id: uuid::Uuid,
    pub artifact_id: ArtifactId,
    pub actor: RepositoryActor,
    pub authority: InvocationAuthority,
    pub labels: BTreeSet<DataLabelId>,
    pub profile: GatewayProfileId,
    pub server: ServerSlug,
    pub task_id: String,
    pub finalized: bool,
    pub request_matches: bool,
}

#[derive(Debug, Clone)]
pub struct ShareLinkDraft {
    pub link_id: ArtifactShareLinkId,
    pub artifact_id: ArtifactId,
    pub actor: RepositoryActor,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub max_downloads: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    Allowed,
    Denied,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ArtifactAuditEvent {
    pub actor: Option<RepositoryActor>,
    pub tenant: Option<TenantId>,
    pub action: String,
    pub artifact_id: Option<ArtifactId>,
    pub outcome: AuditOutcome,
    pub details: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug)]
pub enum RepositoryError {
    Conflict(String),
    Backend(String),
    Corrupt(String),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(message) => write!(formatter, "artifact state conflict: {message}"),
            Self::Backend(message) => write!(formatter, "artifact repository error: {message}"),
            Self::Corrupt(message) => write!(formatter, "artifact state is corrupt: {message}"),
        }
    }
}

impl std::error::Error for RepositoryError {}

/// Authoritative state operations. Counter redemptions are atomic in each implementation.
pub trait ArtifactRepository: Send + Sync {
    fn create_artifact(
        &self,
        artifact: NewArtifact,
    ) -> impl std::future::Future<Output = Result<StoredArtifact, RepositoryError>> + Send;

    fn get_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> impl std::future::Future<Output = Result<Option<StoredArtifact>, RepositoryError>> + Send;

    fn list_artifacts(
        &self,
        query: ArtifactListQuery,
    ) -> impl std::future::Future<Output = Result<Vec<StoredArtifact>, RepositoryError>> + Send;

    fn upsert_grant(
        &self,
        actor: &RepositoryActor,
        grant: Grant,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    fn remove_grant(
        &self,
        artifact_id: ArtifactId,
        subject: &AccessSubject,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    fn set_release_state(
        &self,
        artifact_id: ArtifactId,
        state: ArtifactReleaseState,
    ) -> impl std::future::Future<Output = Result<Option<StoredArtifact>, RepositoryError>> + Send;

    fn create_write_capability(
        &self,
        draft: WriteCapabilityDraft,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    fn reserve_write_capability(
        &self,
        request: WriteCapabilityReservation,
    ) -> impl std::future::Future<Output = Result<Option<RedeemedWriteCapability>, RepositoryError>> + Send;

    fn finalize_write_capability(
        &self,
        redemption_id: uuid::Uuid,
        artifact_id: ArtifactId,
    ) -> impl std::future::Future<Output = Result<bool, RepositoryError>> + Send;

    fn create_share_link(
        &self,
        draft: ShareLinkDraft,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    fn revoke_share_link(
        &self,
        artifact_id: ArtifactId,
        link_id: ArtifactShareLinkId,
    ) -> impl std::future::Future<Output = Result<bool, RepositoryError>> + Send;

    fn redeem_share_link(
        &self,
        token_hash: &str,
    ) -> impl std::future::Future<Output = Result<Option<ArtifactId>, RepositoryError>> + Send;

    fn append_audit(
        &self,
        event: ArtifactAuditEvent,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;
}

pub mod surreal;

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    pub struct InMemoryRepository {
        state: Arc<Mutex<State>>,
    }

    #[derive(Default)]
    struct State {
        artifacts: HashMap<ArtifactId, StoredArtifact>,
        capabilities: HashMap<ArtifactWriteCapabilityId, CapabilityState>,
        redemptions: HashMap<(ArtifactWriteCapabilityId, String), RedemptionState>,
        shares: HashMap<String, ShareState>,
        audits: Vec<ArtifactAuditEvent>,
    }

    struct CapabilityState {
        draft: WriteCapabilityDraft,
        used_artifact_count: u32,
        used_total_bytes: u64,
    }

    #[derive(Clone)]
    struct RedemptionState {
        request_hash: String,
        byte_len: u64,
        redemption_id: uuid::Uuid,
        artifact_id: ArtifactId,
        finalized: bool,
    }

    struct ShareState {
        draft: ShareLinkDraft,
        download_count: u64,
        revoked: bool,
    }

    impl InMemoryRepository {
        pub fn audit_count(&self) -> usize {
            self.state.lock().unwrap().audits.len()
        }
    }

    impl ArtifactRepository for InMemoryRepository {
        async fn create_artifact(
            &self,
            artifact: NewArtifact,
        ) -> Result<StoredArtifact, RepositoryError> {
            let mut state = self.state.lock().unwrap();
            let id = artifact.stored.metadata.artifact_id;
            if state.artifacts.contains_key(&id) {
                return Err(RepositoryError::Conflict("duplicate occurrence id".into()));
            }
            state.artifacts.insert(id, artifact.stored.clone());
            Ok(artifact.stored)
        }

        async fn get_artifact(
            &self,
            artifact_id: ArtifactId,
        ) -> Result<Option<StoredArtifact>, RepositoryError> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .artifacts
                .get(&artifact_id)
                .cloned())
        }

        async fn list_artifacts(
            &self,
            query: ArtifactListQuery,
        ) -> Result<Vec<StoredArtifact>, RepositoryError> {
            let mut artifacts = self
                .state
                .lock()
                .unwrap()
                .artifacts
                .values()
                .filter(|artifact| artifact.tenant == query.actor.tenant)
                .filter(|artifact| {
                    artifact.grants.iter().any(|grant| match &grant.subject {
                        AccessSubject::Principal(user) => user == &query.actor.principal,
                        AccessSubject::Group(group) => query.groups.contains(group),
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            artifacts.sort_by_key(|artifact| std::cmp::Reverse(artifact.metadata.artifact_id));
            if let Some(cursor) = query.cursor {
                artifacts.retain(|artifact| artifact.metadata.artifact_id < cursor);
            }
            artifacts.truncate(query.limit);
            Ok(artifacts)
        }

        async fn upsert_grant(
            &self,
            _actor: &RepositoryActor,
            grant: Grant,
        ) -> Result<(), RepositoryError> {
            let mut state = self.state.lock().unwrap();
            let artifact = state
                .artifacts
                .get_mut(&grant.artifact)
                .ok_or_else(|| RepositoryError::Conflict("artifact does not exist".into()))?;
            artifact
                .grants
                .retain(|existing| existing.subject != grant.subject);
            artifact.grants.push(grant);
            Ok(())
        }

        async fn remove_grant(
            &self,
            artifact_id: ArtifactId,
            subject: &AccessSubject,
        ) -> Result<(), RepositoryError> {
            if let Some(artifact) = self.state.lock().unwrap().artifacts.get_mut(&artifact_id) {
                artifact.grants.retain(|grant| &grant.subject != subject);
            }
            Ok(())
        }

        async fn set_release_state(
            &self,
            artifact_id: ArtifactId,
            release_state: ArtifactReleaseState,
        ) -> Result<Option<StoredArtifact>, RepositoryError> {
            let mut state = self.state.lock().unwrap();
            let Some(artifact) = state.artifacts.get_mut(&artifact_id) else {
                return Ok(None);
            };
            artifact.metadata.release_state = release_state;
            Ok(Some(artifact.clone()))
        }

        async fn create_write_capability(
            &self,
            draft: WriteCapabilityDraft,
        ) -> Result<(), RepositoryError> {
            let mut state = self.state.lock().unwrap();
            if state.capabilities.contains_key(&draft.capability_id) {
                return Err(RepositoryError::Conflict("duplicate capability id".into()));
            }
            state.capabilities.insert(
                draft.capability_id,
                CapabilityState {
                    draft,
                    used_artifact_count: 0,
                    used_total_bytes: 0,
                },
            );
            Ok(())
        }

        async fn reserve_write_capability(
            &self,
            request: WriteCapabilityReservation,
        ) -> Result<Option<RedeemedWriteCapability>, RepositoryError> {
            let WriteCapabilityReservation {
                capability_id,
                token_hash,
                task_id,
                idempotency_key,
                request_hash,
                byte_len,
                requested_labels,
                proposed_artifact_id,
            } = request;
            let mut state = self.state.lock().unwrap();
            let key = (capability_id, idempotency_key);
            if let Some(mut redemption) = state.redemptions.get(&key).cloned() {
                let request_matches =
                    redemption.request_hash == request_hash && redemption.byte_len == byte_len;
                let occurrence_exists = state.artifacts.contains_key(&redemption.artifact_id);
                let capability = state
                    .capabilities
                    .get_mut(&capability_id)
                    .expect("redemption capability remains present");
                if capability.draft.token_hash != token_hash
                    || capability.draft.task_id != task_id
                    || !requested_labels.is_subset(&capability.draft.labels)
                {
                    return Ok(None);
                }
                if !request_matches && !redemption.finalized && !occurrence_exists {
                    let rebound_total = capability
                        .used_total_bytes
                        .checked_sub(redemption.byte_len)
                        .and_then(|used| used.checked_add(byte_len));
                    if rebound_total.is_none_or(|total| total > capability.draft.max_total_bytes) {
                        return Ok(None);
                    }
                    capability.used_total_bytes = rebound_total.expect("validated rebound total");
                    redemption.request_hash = request_hash.clone();
                    redemption.byte_len = byte_len;
                    state.redemptions.insert(key, redemption.clone());
                }
                let capability = state
                    .capabilities
                    .get(&capability_id)
                    .expect("redemption capability remains present");
                return Ok(Some(RedeemedWriteCapability {
                    redemption_id: redemption.redemption_id,
                    artifact_id: redemption.artifact_id,
                    actor: capability.draft.actor.clone(),
                    authority: capability.draft.authority.clone(),
                    labels: capability.draft.labels.clone(),
                    profile: capability.draft.profile.clone(),
                    server: capability.draft.server.clone(),
                    task_id: capability.draft.task_id.clone(),
                    finalized: redemption.finalized,
                    request_matches: redemption.request_hash == request_hash
                        && redemption.byte_len == byte_len,
                }));
            }
            let Some(capability) = state.capabilities.get_mut(&capability_id) else {
                return Ok(None);
            };
            let valid = capability.draft.token_hash == token_hash
                && capability.draft.task_id == task_id
                && capability.draft.expires_at > Utc::now()
                && requested_labels.is_subset(&capability.draft.labels)
                && capability.used_artifact_count < capability.draft.max_artifact_count
                && capability
                    .used_total_bytes
                    .checked_add(byte_len)
                    .is_some_and(|total| total <= capability.draft.max_total_bytes);
            if !valid {
                return Ok(None);
            }
            capability.used_artifact_count += 1;
            capability.used_total_bytes += byte_len;
            let redeemed = RedeemedWriteCapability {
                redemption_id: uuid::Uuid::now_v7(),
                artifact_id: proposed_artifact_id,
                actor: capability.draft.actor.clone(),
                authority: capability.draft.authority.clone(),
                labels: capability.draft.labels.clone(),
                profile: capability.draft.profile.clone(),
                server: capability.draft.server.clone(),
                task_id: capability.draft.task_id.clone(),
                finalized: false,
                request_matches: true,
            };
            state.redemptions.insert(
                key,
                RedemptionState {
                    request_hash,
                    byte_len,
                    redemption_id: redeemed.redemption_id,
                    artifact_id: proposed_artifact_id,
                    finalized: false,
                },
            );
            Ok(Some(redeemed))
        }

        async fn finalize_write_capability(
            &self,
            redemption_id: uuid::Uuid,
            artifact_id: ArtifactId,
        ) -> Result<bool, RepositoryError> {
            let mut state = self.state.lock().unwrap();
            let Some(redemption) = state.redemptions.values_mut().find(|redemption| {
                redemption.redemption_id == redemption_id && redemption.artifact_id == artifact_id
            }) else {
                return Err(RepositoryError::Conflict(
                    "artifact write reservation does not exist".into(),
                ));
            };
            if redemption.finalized {
                return Ok(false);
            }
            redemption.finalized = true;
            Ok(true)
        }

        async fn create_share_link(&self, draft: ShareLinkDraft) -> Result<(), RepositoryError> {
            let mut state = self.state.lock().unwrap();
            if state.shares.contains_key(&draft.token_hash) {
                return Err(RepositoryError::Conflict("duplicate share token".into()));
            }
            state.shares.insert(
                draft.token_hash.clone(),
                ShareState {
                    draft,
                    download_count: 0,
                    revoked: false,
                },
            );
            Ok(())
        }

        async fn revoke_share_link(
            &self,
            artifact_id: ArtifactId,
            link_id: ArtifactShareLinkId,
        ) -> Result<bool, RepositoryError> {
            let mut state = self.state.lock().unwrap();
            let Some(share) = state.shares.values_mut().find(|share| {
                share.draft.artifact_id == artifact_id && share.draft.link_id == link_id
            }) else {
                return Ok(false);
            };
            if share.revoked {
                return Ok(false);
            }
            share.revoked = true;
            Ok(true)
        }

        async fn redeem_share_link(
            &self,
            token_hash: &str,
        ) -> Result<Option<ArtifactId>, RepositoryError> {
            let mut state = self.state.lock().unwrap();
            let Some(share) = state.shares.get(token_hash) else {
                return Ok(None);
            };
            let artifact_id = share.draft.artifact_id;
            let valid_artifact = state.artifacts.get(&artifact_id).is_some_and(|artifact| {
                matches!(
                    artifact.metadata.release_state,
                    ArtifactReleaseState::Releasable | ArtifactReleaseState::Released
                ) && artifact
                    .metadata
                    .compliance
                    .retention_expires_at
                    .is_none_or(|expires| expires > Utc::now())
            });
            let share = state
                .shares
                .get_mut(token_hash)
                .expect("share remains present");
            let valid = valid_artifact
                && !share.revoked
                && share.draft.expires_at > Utc::now()
                && share
                    .draft
                    .max_downloads
                    .is_none_or(|max| share.download_count < max);
            if !valid {
                return Ok(None);
            }
            share.download_count += 1;
            Ok(Some(artifact_id))
        }

        async fn append_audit(&self, event: ArtifactAuditEvent) -> Result<(), RepositoryError> {
            self.state.lock().unwrap().audits.push(event);
            Ok(())
        }
    }
}
