//! Artifact-plane policy enforcement and security workflows.

use std::collections::BTreeSet;
use std::num::NonZeroU64;

use base64::Engine;
use chrono::{TimeDelta, Utc};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::access::{
    AccessDecision, AccessLevel, AccessRequest, AccessSubject, ArtifactId, Grant, decide,
};
use veoveo_mcp_contract::gateway::DataLabelId;
use veoveo_mcp_contract::storage::{
    ArtifactMetadata, ArtifactObject, ArtifactProvenance, ArtifactReleaseState, ComplianceMetadata,
};
use veoveo_mcp_contract::{
    ArtifactAccessRequest, ArtifactAccessRequestId, ArtifactAccessRequestPage,
    ArtifactAccessRequestScope, ArtifactPage, ArtifactPlane, ArtifactPlaneError, ArtifactShareLink,
    ArtifactShareLinkId, ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret,
    CreateArtifactAccessRequest, CreateArtifactShareLinkRequest, DecideArtifactAccessRequest,
    InvocationAuthority, InvocationProvenance, IssueArtifactWriteCapabilityRequest,
    IssuedArtifactWriteCapability, ListArtifactAccessRequests, ListArtifactsRequest, PlaneCaller,
    PutArtifactRequest, RedeemArtifactWriteCapabilityRequest, WorkContextMembershipLevel,
    parse_artifact_plane_uri,
};

use crate::ledger::{
    ArtifactAccessRequestCancellation, ArtifactAccessRequestDecisionDraft,
    ArtifactAccessRequestListQuery, ArtifactAuditEvent, ArtifactListQuery, ArtifactRepository,
    AuditOutcome, BlobSha256, NewArtifact, NewArtifactAccessRequest, RepositoryActor,
    RepositoryError, ShareLinkDraft, StoredArtifact, WriteCapabilityDraft,
    WriteCapabilityReservation,
};
use crate::store::{BlobDownload, BlobStore};

const DEFAULT_INTERNAL_READ_LIMIT: u64 = 64 * 1024 * 1024;
const DEFAULT_REDIRECT_THRESHOLD: u64 = 8 * 1024 * 1024;
const CAPABILITY_MAX_TTL: TimeDelta = TimeDelta::hours(24);
const SHARE_DEFAULT_TTL: TimeDelta = TimeDelta::days(7);
const SHARE_MAX_TTL: TimeDelta = TimeDelta::days(30);
const MAX_ARTIFACT_PUT_JSON_BYTES: usize = 4 * 1024;
const MAX_ARTIFACT_PRESENTATION_FIELD_BYTES: usize = 255;
const DEFAULT_LIST_LIMIT: usize = 50;
const MAX_LIST_LIMIT: usize = 100;
const DEFAULT_ACCESS_REQUEST_LIMIT: usize = 50;
const MAX_ACCESS_REQUEST_LIMIT: usize = 100;
const LIST_SCAN_BATCH: usize = 100;
const OBJECT_KEY_NAMESPACE: uuid::Uuid =
    uuid::Uuid::from_u128(0x78115f34_7753_5b1f_a22c_6cc48885dbf9);

pub enum DownloadDelivery {
    Bytes(Vec<u8>),
    SignedRedirect(String),
    Stream(crate::store::BlobStream),
}

pub struct ArtifactDownload {
    pub metadata: ArtifactMetadata,
    pub delivery: DownloadDelivery,
}

pub struct ArtifactService<R: ArtifactRepository, S: BlobStore> {
    repository: R,
    store: S,
    public_base_url: String,
    max_internal_read_bytes: u64,
    redirect_threshold_bytes: u64,
}

impl<R: ArtifactRepository, S: BlobStore> ArtifactService<R, S> {
    pub fn new(repository: R, store: S) -> Self {
        Self::with_options(
            repository,
            store,
            "http://artifact.invalid",
            DEFAULT_INTERNAL_READ_LIMIT,
            DEFAULT_REDIRECT_THRESHOLD,
        )
    }

    pub fn with_options(
        repository: R,
        store: S,
        public_base_url: impl Into<String>,
        max_internal_read_bytes: u64,
        redirect_threshold_bytes: u64,
    ) -> Self {
        Self {
            repository,
            store,
            public_base_url: public_base_url.into().trim_end_matches('/').to_owned(),
            max_internal_read_bytes,
            redirect_threshold_bytes,
        }
    }

    fn actor(caller: &PlaneCaller) -> Result<RepositoryActor, ArtifactPlaneError> {
        Ok(RepositoryActor {
            tenant: caller
                .tenant()
                .cloned()
                .ok_or(ArtifactPlaneError::Unauthenticated)?,
            principal: caller.identity.actor.id.clone(),
            kind: caller.identity.actor.kind,
            issuer: caller.identity.actor.issuer.clone(),
            subject: caller.identity.actor.subject.clone(),
        })
    }

    async fn load(&self, artifact_id: ArtifactId) -> Result<StoredArtifact, ArtifactPlaneError> {
        let artifact = self
            .repository
            .get_artifact(artifact_id)
            .await
            .map_err(transport)?
            .ok_or(ArtifactPlaneError::NotFound)?;
        if artifact
            .metadata
            .compliance
            .retention_expires_at
            .is_some_and(|expires| expires <= Utc::now())
        {
            return Err(ArtifactPlaneError::NotFound);
        }
        Ok(artifact)
    }

    async fn audit(
        &self,
        actor: Option<RepositoryActor>,
        tenant: Option<veoveo_mcp_contract::TenantId>,
        action: &str,
        artifact_id: Option<ArtifactId>,
        outcome: AuditOutcome,
        details: serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), ArtifactPlaneError> {
        self.repository
            .append_audit(ArtifactAuditEvent {
                actor,
                tenant,
                action: action.to_owned(),
                artifact_id,
                outcome,
                details,
            })
            .await
            .map_err(transport)
    }

    async fn authorize(
        &self,
        caller: &PlaneCaller,
        stored: &StoredArtifact,
        action: &str,
        level: AccessLevel,
    ) -> Result<(), ArtifactPlaneError> {
        let decision = Self::access_decision(caller, stored, level);
        let mut details = serde_json::Map::new();
        details.insert("requested".into(), serde_json::json!(level));
        details.insert("decision".into(), serde_json::json!(decision));
        self.audit(
            Some(Self::actor(caller)?),
            Some(stored.tenant.clone()),
            action,
            Some(stored.metadata.artifact_id),
            if decision.is_allowed() {
                AuditOutcome::Allowed
            } else {
                AuditOutcome::Denied
            },
            details,
        )
        .await?;
        match decision {
            AccessDecision::Allow => Ok(()),
            denied => Err(ArtifactPlaneError::Denied(denied)),
        }
    }

    fn access_decision(
        caller: &PlaneCaller,
        stored: &StoredArtifact,
        level: AccessLevel,
    ) -> AccessDecision {
        let request = AccessRequest {
            caller_id: &caller.identity.actor.id,
            caller_tenant: caller.tenant(),
            caller_labels: caller.clearance(),
            memberships: &caller.memberships,
            artifact_tenant: &stored.tenant,
            artifact_labels: &stored.labels,
            grants: &stored.grants,
            context_membership: (stored.metadata.compliance.work_context.as_ref()
                == Some(&caller.identity.authority.work_context))
            .then_some(caller.identity.authority.membership),
            requested: level,
        };
        decide(&request)
    }

    fn validate_put(
        caller_labels: &BTreeSet<DataLabelId>,
        request: &PutArtifactRequest,
    ) -> Result<BTreeSet<DataLabelId>, ArtifactPlaneError> {
        let request_bytes = serde_json::to_vec(request).map_err(|error| {
            ArtifactPlaneError::InvalidRequest(format!(
                "artifact put descriptor is not serializable: {error}"
            ))
        })?;
        if request_bytes.len() > MAX_ARTIFACT_PUT_JSON_BYTES {
            return Err(ArtifactPlaneError::InvalidRequest(format!(
                "artifact put descriptor exceeds {MAX_ARTIFACT_PUT_JSON_BYTES} bytes"
            )));
        }
        if let Some(mime_type) = &request.mime_type
            && (mime_type.is_empty()
                || mime_type.len() > MAX_ARTIFACT_PRESENTATION_FIELD_BYTES
                || mime_type.trim() != mime_type
                || mime_type.chars().any(char::is_control)
                || !mime_type.contains('/'))
        {
            return Err(ArtifactPlaneError::InvalidRequest(
                "artifact MIME type must be a trimmed media type of at most 255 bytes".into(),
            ));
        }
        if let Some(filename) = &request.filename
            && (filename.is_empty()
                || filename.len() > MAX_ARTIFACT_PRESENTATION_FIELD_BYTES
                || filename.trim() != filename
                || filename.chars().any(char::is_control)
                || filename.contains('/')
                || filename.contains('\\')
                || matches!(filename.as_str(), "." | ".."))
        {
            return Err(ArtifactPlaneError::InvalidRequest(
                "artifact filename must be a trimmed basename of at most 255 bytes".into(),
            ));
        }
        if !request.metadata.is_null() && !request.metadata.is_object() {
            return Err(ArtifactPlaneError::InvalidRequest(
                "artifact metadata must be an object or null".into(),
            ));
        }
        if request
            .retention_expires_at
            .is_some_and(|expires| expires <= Utc::now())
        {
            return Err(ArtifactPlaneError::InvalidRequest(
                "artifact retention expiry must be in the future".into(),
            ));
        }
        let labels = request.effective_labels();
        if !labels.is_subset(caller_labels) {
            return Err(ArtifactPlaneError::InvalidRequest(
                "artifact labels exceed the writer's clearance".into(),
            ));
        }
        Ok(labels)
    }

    async fn put_for_actor(
        &self,
        actor: RepositoryActor,
        authority: InvocationAuthority,
        allowed_labels: &BTreeSet<DataLabelId>,
        request: PutArtifactRequest,
        bytes: Vec<u8>,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let artifact_id = ArtifactId::new();
        let stored = self
            .store_occurrence(
                artifact_id,
                actor.clone(),
                authority,
                allowed_labels,
                request,
                bytes,
            )
            .await?;
        self.audit(
            Some(actor),
            Some(stored.tenant.clone()),
            "artifact.put",
            Some(artifact_id),
            AuditOutcome::Allowed,
            serde_json::Map::new(),
        )
        .await?;
        Ok(stored.metadata)
    }

    async fn store_occurrence(
        &self,
        artifact_id: ArtifactId,
        actor: RepositoryActor,
        authority: InvocationAuthority,
        allowed_labels: &BTreeSet<DataLabelId>,
        mut request: PutArtifactRequest,
        bytes: Vec<u8>,
    ) -> Result<StoredArtifact, ArtifactPlaneError> {
        if request.classification.is_none() {
            request.classification = authority.output_policy.classification.clone();
        }
        request
            .data_labels
            .extend(authority.output_policy.data_labels.iter().cloned());
        let labels = Self::validate_put(allowed_labels, &request)?;
        let sha = compute_sha(&bytes);
        let object_key = tenant_blob_key(&actor.tenant, &sha);
        self.store
            .put(&object_key, bytes.clone())
            .await
            .map_err(transport)?;
        let metadata = ArtifactMetadata {
            artifact_id,
            byte_len: bytes.len() as u64,
            mime_type: request.mime_type.clone(),
            filename: request.filename.clone(),
            artifact_uri: artifact_id.plane_uri(),
            download_url: None,
            created_at: Utc::now(),
            release_state: ArtifactReleaseState::Private,
            compliance: ComplianceMetadata {
                classification: request.classification.clone(),
                tenant_id: Some(actor.tenant.clone()),
                owner: Some(authority.output_policy.owner.clone()),
                work_context: Some(authority.work_context.clone()),
                provenance: Some(ArtifactProvenance {
                    producer: actor.principal.clone(),
                    invocation_mode: authority.provenance.mode(),
                    initiator: authority.provenance.initiator().cloned(),
                    delegation_id: match &authority.provenance {
                        InvocationProvenance::Delegated { delegation_id, .. } => {
                            Some(delegation_id.clone())
                        }
                        InvocationProvenance::Direct { .. } | InvocationProvenance::Automated => {
                            None
                        }
                    },
                    policy_revision: authority.policy_revision.clone(),
                }),
                data_labels: request.data_labels.clone(),
                retention_expires_at: request.retention_expires_at,
            },
            metadata: request.metadata,
        };
        let mut grants = vec![Grant {
            artifact: artifact_id,
            subject: authority.output_policy.owner.clone(),
            level: AccessLevel::Admin,
            tenant: actor.tenant.clone(),
            data_labels: labels.clone(),
            retention_expires_at: request.retention_expires_at,
        }];
        for initial in &authority.output_policy.initial_grants {
            if let Some(existing) = grants
                .iter_mut()
                .find(|grant| grant.subject == initial.subject)
            {
                existing.level = existing.level.max(initial.level);
            } else {
                grants.push(Grant {
                    artifact: artifact_id,
                    subject: initial.subject.clone(),
                    level: initial.level,
                    tenant: actor.tenant.clone(),
                    data_labels: labels.clone(),
                    retention_expires_at: request.retention_expires_at,
                });
            }
        }
        self.repository
            .create_artifact(NewArtifact {
                actor: actor.clone(),
                stored: StoredArtifact {
                    metadata,
                    tenant: actor.tenant.clone(),
                    labels,
                    grants,
                    authority,
                    blob_sha256: sha,
                    object_key,
                },
            })
            .await
            .map_err(transport)
    }

    pub async fn issue_write_capability(
        &self,
        caller: &PlaneCaller,
        request: IssueArtifactWriteCapabilityRequest,
    ) -> Result<IssuedArtifactWriteCapability, ArtifactPlaneError> {
        let now = Utc::now();
        if uuid::Uuid::parse_str(&request.task_id)
            .ok()
            .is_none_or(|task_id| task_id.get_version_num() != 7)
        {
            return Err(ArtifactPlaneError::InvalidRequest(
                "capability task_id must be a UUIDv7".into(),
            ));
        }
        if request.expires_at <= now || request.expires_at > now + CAPABILITY_MAX_TTL {
            return Err(ArtifactPlaneError::InvalidRequest(
                "capability expiry must be within the next 24 hours".into(),
            ));
        }
        let actor = Self::actor(caller)?;
        let capability_id = ArtifactWriteCapabilityId::new();
        let secret = random_secret()?;
        self.repository
            .create_write_capability(WriteCapabilityDraft {
                capability_id,
                actor: actor.clone(),
                authority: caller.identity.authority.clone(),
                profile: caller.identity.profile.clone(),
                server: caller.identity.server.clone(),
                task_id: request.task_id.clone(),
                token_hash: secret_hash(b"veoveo.artifact-write.v1", &secret),
                labels: caller.clearance().clone(),
                max_artifact_count: request.max_artifact_count.get(),
                max_total_bytes: request.max_total_bytes.get(),
                expires_at: request.expires_at,
            })
            .await
            .map_err(transport)?;
        self.audit(
            Some(actor.clone()),
            Some(actor.tenant),
            "artifact.capability.issue",
            None,
            AuditOutcome::Allowed,
            serde_json::Map::from_iter([(
                "capability_id".into(),
                serde_json::json!(capability_id),
            )]),
        )
        .await?;
        Ok(IssuedArtifactWriteCapability {
            capability_id,
            secret: ArtifactWriteCapabilitySecret::new(secret)?,
            task_id: request.task_id,
            expires_at: request.expires_at,
        })
    }

    pub async fn redeem_write_capability(
        &self,
        secret: &str,
        request: RedeemArtifactWriteCapabilityRequest,
        bytes: Vec<u8>,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let requested_labels =
            Self::validate_put(&request.artifact.effective_labels(), &request.artifact)?;
        let sha = compute_sha(&bytes);
        let request_hash = artifact_write_request_hash(&request.artifact, &sha)?;
        let proposed_artifact_id = ArtifactId::new();
        let redemption = self
            .repository
            .reserve_write_capability(WriteCapabilityReservation {
                capability_id: request.capability_id,
                token_hash: secret_hash(b"veoveo.artifact-write.v1", secret),
                task_id: request.task_id.clone(),
                idempotency_key: request.idempotency_key.to_string(),
                request_hash,
                byte_len: bytes.len() as u64,
                requested_labels,
                proposed_artifact_id,
            })
            .await
            .map_err(|error| match error {
                crate::ledger::RepositoryError::Conflict(message) => {
                    ArtifactPlaneError::Conflict(message)
                }
                other => transport(other),
            })?
            .ok_or(ArtifactPlaneError::Unauthenticated)?;
        if redemption.finalized {
            return self
                .repository
                .get_artifact(redemption.artifact_id)
                .await
                .map_err(transport)?
                .map(|stored| stored.metadata)
                .ok_or_else(|| {
                    ArtifactPlaneError::Transport(
                        "finalized artifact write has no occurrence".into(),
                    )
                });
        }
        if !redemption.request_matches {
            let existing = self
                .repository
                .get_artifact(redemption.artifact_id)
                .await
                .map_err(transport)?;
            let Some(existing) = existing else {
                return Err(ArtifactPlaneError::Conflict(
                    "artifact write reservation changed concurrently before staging".into(),
                ));
            };
            if existing.tenant != redemption.actor.tenant {
                return Err(ArtifactPlaneError::Conflict(
                    "reserved occurrence belongs to a different tenant".into(),
                ));
            }
            let finalized = self
                .repository
                .finalize_write_capability(redemption.redemption_id, redemption.artifact_id)
                .await
                .map_err(transport)?;
            if finalized {
                self.audit(
                    Some(redemption.actor),
                    Some(existing.tenant.clone()),
                    "artifact.capability.redeem",
                    Some(redemption.artifact_id),
                    AuditOutcome::Allowed,
                    serde_json::Map::from_iter([(
                        "idempotency_key".into(),
                        serde_json::json!(request.idempotency_key.as_str()),
                    )]),
                )
                .await?;
            }
            return Ok(existing.metadata);
        }

        let stored = match self
            .store_occurrence(
                redemption.artifact_id,
                redemption.actor.clone(),
                redemption.authority.clone(),
                &redemption.labels,
                request.artifact,
                bytes,
            )
            .await
        {
            Ok(stored) => stored,
            Err(ArtifactPlaneError::Transport(_)) => {
                let existing = self
                    .repository
                    .get_artifact(redemption.artifact_id)
                    .await
                    .map_err(transport)?;
                let Some(existing) = existing else {
                    return Err(ArtifactPlaneError::Transport(
                        "artifact occurrence staging failed".into(),
                    ));
                };
                if existing.tenant != redemption.actor.tenant
                    || existing.blob_sha256.as_str() != sha.as_str()
                {
                    return Err(ArtifactPlaneError::Conflict(
                        "reserved occurrence conflicts with the idempotent artifact write".into(),
                    ));
                }
                existing
            }
            Err(error) => return Err(error),
        };
        let finalized = self
            .repository
            .finalize_write_capability(redemption.redemption_id, redemption.artifact_id)
            .await
            .map_err(transport)?;
        if finalized {
            self.audit(
                Some(redemption.actor),
                Some(stored.tenant.clone()),
                "artifact.capability.redeem",
                Some(redemption.artifact_id),
                AuditOutcome::Allowed,
                serde_json::Map::from_iter([(
                    "idempotency_key".into(),
                    serde_json::json!(request.idempotency_key.as_str()),
                )]),
            )
            .await?;
        }
        Ok(stored.metadata)
    }

    pub async fn download(
        &self,
        caller: &PlaneCaller,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactDownload, ArtifactPlaneError> {
        let stored = self.load(artifact_id).await?;
        self.authorize(caller, &stored, "artifact.download", AccessLevel::Read)
            .await?;
        self.delivery(stored).await
    }

    pub async fn redeem_public_share(
        &self,
        token: &str,
    ) -> Result<ArtifactDownload, ArtifactPlaneError> {
        if token.len() < 32 || token.chars().any(char::is_whitespace) {
            return Err(ArtifactPlaneError::NotFound);
        }
        let token_hash = secret_hash(b"veoveo.artifact-share.v1", token);
        let artifact_id = self
            .repository
            .redeem_share_link(&token_hash)
            .await
            .map_err(transport)?;
        let Some(artifact_id) = artifact_id else {
            self.audit(
                None,
                None,
                "artifact.share.redeem",
                None,
                AuditOutcome::Denied,
                serde_json::Map::new(),
            )
            .await?;
            return Err(ArtifactPlaneError::NotFound);
        };
        let stored = self.load(artifact_id).await?;
        self.audit(
            None,
            Some(stored.tenant.clone()),
            "artifact.share.redeem",
            Some(artifact_id),
            AuditOutcome::Allowed,
            serde_json::Map::new(),
        )
        .await?;
        self.delivery(stored).await
    }

    async fn delivery(
        &self,
        stored: StoredArtifact,
    ) -> Result<ArtifactDownload, ArtifactPlaneError> {
        let delivery = if stored.metadata.byte_len < self.redirect_threshold_bytes {
            DownloadDelivery::Bytes(
                self.store
                    .get_bounded(&stored.object_key, self.redirect_threshold_bytes)
                    .await
                    .map_err(transport)?,
            )
        } else {
            match self
                .store
                .download(&stored.object_key)
                .await
                .map_err(transport)?
            {
                BlobDownload::SignedRedirect(url) => DownloadDelivery::SignedRedirect(url),
                BlobDownload::Stream(stream) => DownloadDelivery::Stream(stream),
            }
        };
        Ok(ArtifactDownload {
            metadata: stored.metadata,
            delivery,
        })
    }
}

impl<R: ArtifactRepository, S: BlobStore> ArtifactPlane for ArtifactService<R, S> {
    async fn put(
        &self,
        caller: &PlaneCaller,
        request: PutArtifactRequest,
        bytes: Vec<u8>,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        self.put_for_actor(
            Self::actor(caller)?,
            caller.identity.authority.clone(),
            caller.clearance(),
            request,
            bytes,
        )
        .await
    }

    async fn get(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        level: AccessLevel,
    ) -> Result<ArtifactObject, ArtifactPlaneError> {
        let stored = self.load(*artifact_id).await?;
        self.authorize(caller, &stored, "artifact.get", level)
            .await?;
        let bytes = self
            .store
            .get_bounded(&stored.object_key, self.max_internal_read_bytes)
            .await
            .map_err(|error| match error {
                crate::store::BlobStoreError::TooLarge { .. } => {
                    ArtifactPlaneError::InvalidRequest(
                        "artifact exceeds the internal read limit; use the download endpoint"
                            .into(),
                    )
                }
                other => transport(other),
            })?;
        Ok(ArtifactObject {
            metadata: stored.metadata,
            bytes,
        })
    }

    async fn head(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let stored = self.load(*artifact_id).await?;
        self.authorize(caller, &stored, "artifact.head", AccessLevel::Read)
            .await?;
        Ok(stored.metadata)
    }

    async fn list(
        &self,
        caller: &PlaneCaller,
        request: ListArtifactsRequest,
    ) -> Result<ArtifactPage, ArtifactPlaneError> {
        let limit = usize::from(request.limit.unwrap_or(DEFAULT_LIST_LIMIT as u16));
        if limit == 0 || limit > MAX_LIST_LIMIT {
            return Err(ArtifactPlaneError::InvalidRequest(format!(
                "artifact list limit must be between 1 and {MAX_LIST_LIMIT}"
            )));
        }
        let actor = Self::actor(caller)?;
        let groups: BTreeSet<_> = caller
            .memberships
            .iter()
            .map(|membership| membership.group.clone())
            .collect();
        let mut scan_cursor = request.cursor;
        let mut artifacts = Vec::with_capacity(limit);

        loop {
            let candidates = self
                .repository
                .list_artifacts(ArtifactListQuery {
                    actor: actor.clone(),
                    groups: groups.clone(),
                    cursor: scan_cursor,
                    limit: LIST_SCAN_BATCH,
                })
                .await
                .map_err(transport)?;
            if candidates.is_empty() {
                break;
            }
            let exhausted = candidates.len() < LIST_SCAN_BATCH;
            let previous_cursor = scan_cursor;
            for stored in candidates {
                scan_cursor = Some(stored.metadata.artifact_id);
                let retained = stored
                    .metadata
                    .compliance
                    .retention_expires_at
                    .is_none_or(|expires| expires > Utc::now());
                if retained
                    && Self::access_decision(caller, &stored, AccessLevel::Read).is_allowed()
                {
                    artifacts.push(stored.metadata);
                    if artifacts.len() == limit {
                        break;
                    }
                }
            }
            if artifacts.len() == limit || exhausted {
                break;
            }
            if scan_cursor == previous_cursor {
                return Err(ArtifactPlaneError::Transport(
                    "artifact discovery cursor did not advance".into(),
                ));
            }
        }

        self.audit(
            Some(actor.clone()),
            Some(actor.tenant),
            "artifact.list",
            None,
            AuditOutcome::Allowed,
            serde_json::Map::from_iter([
                ("count".into(), serde_json::json!(artifacts.len())),
                ("limit".into(), serde_json::json!(limit)),
            ]),
        )
        .await?;
        let next_cursor = (artifacts.len() == limit).then(|| {
            artifacts
                .last()
                .expect("full page has a last artifact")
                .artifact_id
        });
        Ok(ArtifactPage {
            artifacts,
            next_cursor,
        })
    }

    async fn resolve(
        &self,
        caller: &PlaneCaller,
        uri: &str,
    ) -> Result<ArtifactObject, ArtifactPlaneError> {
        let artifact_id = parse_artifact_plane_uri(uri).ok_or_else(|| {
            ArtifactPlaneError::InvalidRequest(format!("invalid artifact URI `{uri}`"))
        })?;
        self.get(caller, &artifact_id, AccessLevel::Read).await
    }

    async fn grant(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        subject: AccessSubject,
        level: AccessLevel,
    ) -> Result<(), ArtifactPlaneError> {
        let stored = self.load(*artifact_id).await?;
        self.authorize(caller, &stored, "artifact.grant", AccessLevel::Admin)
            .await?;
        if matches!(
            &subject,
            owner
                if stored.metadata.compliance.owner.as_ref() == Some(owner)
                    && level != AccessLevel::Admin
        ) {
            return Err(ArtifactPlaneError::Conflict(
                "the owner admin grant cannot be lowered".into(),
            ));
        }
        self.repository
            .upsert_grant(
                &Self::actor(caller)?,
                Grant {
                    artifact: *artifact_id,
                    subject,
                    level,
                    tenant: stored.tenant,
                    data_labels: stored.labels,
                    retention_expires_at: stored.metadata.compliance.retention_expires_at,
                },
            )
            .await
            .map_err(transport)
    }

    async fn revoke(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        subject: &AccessSubject,
    ) -> Result<(), ArtifactPlaneError> {
        let stored = self.load(*artifact_id).await?;
        self.authorize(caller, &stored, "artifact.revoke", AccessLevel::Admin)
            .await?;
        if matches!(
            subject,
            owner if stored.metadata.compliance.owner.as_ref() == Some(owner)
        ) {
            return Err(ArtifactPlaneError::Conflict(
                "the owner admin grant cannot be revoked".into(),
            ));
        }
        self.repository
            .remove_grant(*artifact_id, subject)
            .await
            .map_err(transport)
    }

    async fn list_grants(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
    ) -> Result<Vec<Grant>, ArtifactPlaneError> {
        let stored = self.load(*artifact_id).await?;
        self.authorize(caller, &stored, "artifact.grants.list", AccessLevel::Admin)
            .await?;
        Ok(stored.grants)
    }

    async fn set_release_state(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        release_state: ArtifactReleaseState,
    ) -> Result<ArtifactMetadata, ArtifactPlaneError> {
        let stored = self.load(*artifact_id).await?;
        self.authorize(caller, &stored, "artifact.release", AccessLevel::Admin)
            .await?;
        self.repository
            .set_release_state(*artifact_id, release_state)
            .await
            .map_err(transport)?
            .map(|stored| stored.metadata)
            .ok_or(ArtifactPlaneError::NotFound)
    }

    async fn create_share_link(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        request: CreateArtifactShareLinkRequest,
    ) -> Result<ArtifactShareLink, ArtifactPlaneError> {
        let stored = self.load(*artifact_id).await?;
        self.authorize(caller, &stored, "artifact.share.create", AccessLevel::Admin)
            .await?;
        if stored.metadata.release_state == ArtifactReleaseState::Private {
            return Err(ArtifactPlaneError::Conflict(
                "artifact must be releasable before a public link can be created".into(),
            ));
        }
        let now = Utc::now();
        let expires_at = request.expires_at.unwrap_or(now + SHARE_DEFAULT_TTL);
        if expires_at <= now || expires_at > now + SHARE_MAX_TTL {
            return Err(ArtifactPlaneError::InvalidRequest(
                "share expiry must be within the next 30 days".into(),
            ));
        }
        let actor = Self::actor(caller)?;
        let link_id = ArtifactShareLinkId::new();
        let secret = random_secret()?;
        let max_downloads = request.max_downloads.map(NonZeroU64::get);
        self.repository
            .create_share_link(ShareLinkDraft {
                link_id,
                artifact_id: *artifact_id,
                actor: actor.clone(),
                token_hash: secret_hash(b"veoveo.artifact-share.v1", &secret),
                expires_at,
                max_downloads,
            })
            .await
            .map_err(transport)?;
        Ok(ArtifactShareLink {
            link_id,
            artifact_id: *artifact_id,
            url: format!("{}/s/{secret}", self.public_base_url),
            expires_at,
            max_downloads: request.max_downloads,
        })
    }

    async fn revoke_share_link(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        link_id: &ArtifactShareLinkId,
    ) -> Result<(), ArtifactPlaneError> {
        let stored = self.load(*artifact_id).await?;
        self.authorize(caller, &stored, "artifact.share.revoke", AccessLevel::Admin)
            .await?;
        if self
            .repository
            .revoke_share_link(*artifact_id, *link_id)
            .await
            .map_err(transport)?
        {
            Ok(())
        } else {
            Err(ArtifactPlaneError::NotFound)
        }
    }

    async fn create_access_request(
        &self,
        caller: &PlaneCaller,
        artifact_id: &ArtifactId,
        request: CreateArtifactAccessRequest,
    ) -> Result<ArtifactAccessRequest, ArtifactPlaneError> {
        let stored = self.load(*artifact_id).await?;
        let actor = Self::actor(caller)?;
        if stored.tenant != actor.tenant {
            return Err(ArtifactPlaneError::NotFound);
        }
        match Self::access_decision(caller, &stored, request.requested_level) {
            AccessDecision::Allow => {
                return Err(ArtifactPlaneError::Conflict(
                    "the requested access is already effective".into(),
                ));
            }
            AccessDecision::DenyTenant => return Err(ArtifactPlaneError::NotFound),
            AccessDecision::DenyClearance => {
                return Err(ArtifactPlaneError::Denied(AccessDecision::DenyClearance));
            }
            AccessDecision::DenyNeedToKnow => {}
        }
        let created = self
            .repository
            .create_or_reopen_access_request(NewArtifactAccessRequest {
                request_id: ArtifactAccessRequestId::new(),
                actor: actor.clone(),
                artifact_id: *artifact_id,
                requested_level: request.requested_level,
                justification: request.justification,
            })
            .await
            .map_err(repository_mutation_error)?;
        self.audit(
            Some(actor.clone()),
            Some(actor.tenant),
            "artifact.access_request.create",
            Some(*artifact_id),
            AuditOutcome::Allowed,
            serde_json::Map::from_iter([
                (
                    "request_id".to_owned(),
                    serde_json::json!(created.id.to_string()),
                ),
                (
                    "requested_level".to_owned(),
                    serde_json::json!(created.requested_level),
                ),
            ]),
        )
        .await?;
        Ok(created)
    }

    async fn list_access_requests(
        &self,
        caller: &PlaneCaller,
        request: ListArtifactAccessRequests,
    ) -> Result<ArtifactAccessRequestPage, ArtifactPlaneError> {
        let limit = usize::from(request.limit.unwrap_or(DEFAULT_ACCESS_REQUEST_LIMIT as u16));
        if limit == 0 || limit > MAX_ACCESS_REQUEST_LIMIT {
            return Err(ArtifactPlaneError::InvalidRequest(format!(
                "access request list limit must be between 1 and {MAX_ACCESS_REQUEST_LIMIT}"
            )));
        }
        let actor = Self::actor(caller)?;
        let scope = request.scope.unwrap_or(ArtifactAccessRequestScope::Mine);
        let work_context = match scope {
            ArtifactAccessRequestScope::Mine => None,
            ArtifactAccessRequestScope::Reviewable => {
                if !caller
                    .identity
                    .authority
                    .membership
                    .allows(WorkContextMembershipLevel::Custodian)
                {
                    return Err(ArtifactPlaneError::Denied(AccessDecision::DenyNeedToKnow));
                }
                Some(caller.identity.authority.work_context.clone())
            }
        };
        let mut requests = self
            .repository
            .list_access_requests(ArtifactAccessRequestListQuery {
                actor: actor.clone(),
                work_context,
                state: request.state,
                cursor: request.cursor,
                limit: limit + 1,
            })
            .await
            .map_err(transport)?;
        let next_cursor = (requests.len() > limit).then(|| requests[limit - 1].id);
        requests.truncate(limit);
        self.audit(
            Some(actor.clone()),
            Some(actor.tenant),
            "artifact.access_request.list",
            None,
            AuditOutcome::Allowed,
            serde_json::Map::from_iter([
                ("scope".to_owned(), serde_json::json!(scope)),
                ("count".to_owned(), serde_json::json!(requests.len())),
            ]),
        )
        .await?;
        Ok(ArtifactAccessRequestPage {
            requests,
            next_cursor,
        })
    }

    async fn decide_access_request(
        &self,
        caller: &PlaneCaller,
        request_id: &ArtifactAccessRequestId,
        decision: DecideArtifactAccessRequest,
    ) -> Result<ArtifactAccessRequest, ArtifactPlaneError> {
        let actor = Self::actor(caller)?;
        let request = self
            .repository
            .get_access_request(&actor, *request_id)
            .await
            .map_err(transport)?
            .ok_or(ArtifactPlaneError::NotFound)?;
        let stored = self.load(request.artifact_id).await?;
        if request.work_context != stored.authority.work_context {
            return Err(ArtifactPlaneError::Transport(
                "access request Work Context differs from its artifact".into(),
            ));
        }
        self.authorize(
            caller,
            &stored,
            "artifact.access_request.decide",
            AccessLevel::Admin,
        )
        .await?;
        let decided = self
            .repository
            .decide_access_request(ArtifactAccessRequestDecisionDraft {
                actor: actor.clone(),
                request_id: *request_id,
                decision: decision.decision,
                note: decision.note,
            })
            .await
            .map_err(repository_mutation_error)?;
        self.audit(
            Some(actor.clone()),
            Some(actor.tenant),
            "artifact.access_request.decision",
            Some(decided.artifact_id),
            AuditOutcome::Allowed,
            serde_json::Map::from_iter([
                (
                    "request_id".to_owned(),
                    serde_json::json!(request_id.to_string()),
                ),
                ("state".to_owned(), serde_json::json!(decided.state)),
            ]),
        )
        .await?;
        Ok(decided)
    }

    async fn cancel_access_request(
        &self,
        caller: &PlaneCaller,
        request_id: &ArtifactAccessRequestId,
    ) -> Result<ArtifactAccessRequest, ArtifactPlaneError> {
        let actor = Self::actor(caller)?;
        let cancelled = self
            .repository
            .cancel_access_request(ArtifactAccessRequestCancellation {
                actor: actor.clone(),
                request_id: *request_id,
            })
            .await
            .map_err(repository_mutation_error)?;
        self.audit(
            Some(actor.clone()),
            Some(actor.tenant),
            "artifact.access_request.cancel",
            Some(cancelled.artifact_id),
            AuditOutcome::Allowed,
            serde_json::Map::from_iter([(
                "request_id".to_owned(),
                serde_json::json!(request_id.to_string()),
            )]),
        )
        .await?;
        Ok(cancelled)
    }
}

fn compute_sha(bytes: &[u8]) -> BlobSha256 {
    BlobSha256::new(hex::encode(Sha256::digest(bytes)))
        .expect("a SHA-256 digest is valid lowercase hex")
}

fn artifact_write_request_hash(
    request: &PutArtifactRequest,
    blob_sha: &BlobSha256,
) -> Result<String, ArtifactPlaneError> {
    let request = serde_json::to_vec(request).map_err(|error| {
        ArtifactPlaneError::InvalidRequest(format!(
            "artifact write request cannot be canonicalized: {error}"
        ))
    })?;
    let mut hash = Sha256::new();
    hash.update(b"veoveo.artifact-write-request.v1");
    hash.update([0]);
    hash.update(blob_sha.as_str().as_bytes());
    hash.update([0]);
    hash.update(request);
    Ok(hex::encode(hash.finalize()))
}

fn tenant_blob_key(tenant: &veoveo_mcp_contract::TenantId, sha: &BlobSha256) -> String {
    let blob_id = uuid::Uuid::new_v5(
        &OBJECT_KEY_NAMESPACE,
        format!("{}:{}", tenant.as_str(), sha.as_str()).as_bytes(),
    );
    format!("tenants/{}/blobs/{blob_id}", tenant.as_str())
}

fn random_secret() -> Result<String, ArtifactPlaneError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| {
        ArtifactPlaneError::Transport(format!("randomness unavailable: {error}"))
    })?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn secret_hash(domain: &[u8], secret: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update([0]);
    hash.update(secret.as_bytes());
    hex::encode(hash.finalize())
}

fn transport(error: impl std::fmt::Display) -> ArtifactPlaneError {
    ArtifactPlaneError::Transport(error.to_string())
}

fn repository_mutation_error(error: RepositoryError) -> ArtifactPlaneError {
    match error {
        RepositoryError::Conflict(message) => ArtifactPlaneError::Conflict(message),
        other => transport(other),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::num::{NonZeroU32, NonZeroU64};

    use chrono::{TimeDelta, Utc};
    use veoveo_mcp_contract::gateway::{
        GatewayProfileId, PrincipalId, PrincipalKind, ServerSlug, TenantId, TokenIssuer,
        TokenSubject,
    };
    use veoveo_mcp_contract::internal_auth::GatewayInternalIdentity;
    use veoveo_mcp_contract::{
        AccessSubject, ArtifactAccessRequestDecision, ArtifactAccessRequestScope,
        ArtifactAccessRequestState, ArtifactWriteIdempotencyKey, InvocationProvenance, JwtId,
        PolicyVersion, Principal, WorkContextId, WorkContextMembershipLevel,
        WorkContextOutputPolicy,
    };

    use super::*;
    use crate::ledger::testing::InMemoryRepository;
    use crate::store::testing::InMemoryBlobStore;

    fn caller(principal: &str, tenant: &str, labels: &[&str]) -> PlaneCaller {
        let now = Utc::now();
        let actor = Principal {
            id: PrincipalId::new(principal).unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            subject: TokenSubject::new(format!("subject-{principal}")).unwrap(),
            tenant: Some(TenantId::new(tenant).unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: labels
                .iter()
                .map(|label| DataLabelId::new(*label).unwrap())
                .collect(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(now),
        };
        PlaneCaller {
            bearer_token: "signed-token".into(),
            identity: GatewayInternalIdentity {
                issuer: TokenIssuer::new("veoveo-internal").unwrap(),
                profile: GatewayProfileId::new("operator").unwrap(),
                server: ServerSlug::new("media").unwrap(),
                actor: actor.clone(),
                authority: InvocationAuthority {
                    work_context: WorkContextId::new("mission").unwrap(),
                    tenant: TenantId::new(tenant).unwrap(),
                    membership: WorkContextMembershipLevel::Owner,
                    policy_revision: PolicyVersion::new("r1").unwrap(),
                    output_policy: WorkContextOutputPolicy {
                        owner: AccessSubject::Principal(actor.id.clone()),
                        initial_grants: Vec::new(),
                        classification: None,
                        data_labels: BTreeSet::new(),
                    },
                    provenance: InvocationProvenance::Direct {
                        initiator: actor.id.clone(),
                    },
                },
                jwt_id: JwtId::new(uuid::Uuid::new_v4().to_string()).unwrap(),
                issued_at: now,
                not_before: now,
                expires_at: now + TimeDelta::minutes(5),
            },
            memberships: BTreeSet::new(),
        }
    }

    fn service() -> (
        ArtifactService<InMemoryRepository, InMemoryBlobStore>,
        InMemoryRepository,
    ) {
        let repository = InMemoryRepository::default();
        (
            ArtifactService::with_options(
                repository.clone(),
                InMemoryBlobStore::default(),
                "https://artifacts.example.com",
                1024,
                8,
            ),
            repository,
        )
    }

    #[tokio::test]
    async fn put_rejects_transport_invalid_presentation_metadata() {
        let (service, _) = service();
        let alice = caller("alice", "acme", &[]);
        for request in [
            PutArtifactRequest {
                mime_type: Some("not-a-media-type".into()),
                ..PutArtifactRequest::default()
            },
            PutArtifactRequest {
                filename: Some("../escape.bin".into()),
                ..PutArtifactRequest::default()
            },
            PutArtifactRequest {
                metadata: serde_json::json!({"value": "x".repeat(5_000)}),
                ..PutArtifactRequest::default()
            },
        ] {
            assert!(matches!(
                service.put(&alice, request, b"data".to_vec()).await,
                Err(ArtifactPlaneError::InvalidRequest(_))
            ));
        }
    }

    #[tokio::test]
    async fn identical_bytes_create_distinct_occurrences_and_tenants_remain_isolated() {
        let (service, repository) = service();
        let alice = caller("alice", "acme", &[]);
        let first = service
            .put(&alice, PutArtifactRequest::default(), b"same".to_vec())
            .await
            .unwrap();
        let second = service
            .put(&alice, PutArtifactRequest::default(), b"same".to_vec())
            .await
            .unwrap();
        assert_ne!(first.artifact_id, second.artifact_id);
        assert_eq!(first.artifact_uri, first.artifact_id.plane_uri());
        assert_eq!(first.download_url, None);
        assert_eq!(
            service
                .get(&alice, &first.artifact_id, AccessLevel::Read)
                .await
                .unwrap()
                .bytes,
            b"same"
        );

        let other_tenant = caller("alice", "beta", &[]);
        assert_eq!(
            service
                .get(&other_tenant, &first.artifact_id, AccessLevel::Read)
                .await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyTenant))
        );
        assert!(repository.audit_count() >= 4);
    }

    #[tokio::test]
    async fn discovery_is_grant_filtered_and_keyset_paginated() {
        let (service, _) = service();
        let alice = caller("alice", "acme", &[]);
        let bob = caller("bob", "acme", &[]);
        let first = service
            .put(&alice, PutArtifactRequest::default(), b"first".to_vec())
            .await
            .unwrap();
        let second = service
            .put(&alice, PutArtifactRequest::default(), b"second".to_vec())
            .await
            .unwrap();
        let third = service
            .put(&alice, PutArtifactRequest::default(), b"third".to_vec())
            .await
            .unwrap();

        assert!(
            service
                .list(&bob, ListArtifactsRequest::default())
                .await
                .unwrap()
                .artifacts
                .is_empty()
        );
        service
            .grant(
                &alice,
                &first.artifact_id,
                AccessSubject::Principal(bob.identity.actor.id.clone()),
                AccessLevel::Read,
            )
            .await
            .unwrap();
        let bob_page = service
            .list(&bob, ListArtifactsRequest::default())
            .await
            .unwrap();
        assert_eq!(
            bob_page
                .artifacts
                .iter()
                .map(|artifact| artifact.artifact_id)
                .collect::<Vec<_>>(),
            vec![first.artifact_id]
        );

        let first_page = service
            .list(
                &alice,
                ListArtifactsRequest {
                    cursor: None,
                    limit: Some(2),
                },
            )
            .await
            .unwrap();
        assert_eq!(first_page.artifacts.len(), 2);
        let next = first_page.next_cursor.expect("full page has a cursor");
        let second_page = service
            .list(
                &alice,
                ListArtifactsRequest {
                    cursor: Some(next),
                    limit: Some(2),
                },
            )
            .await
            .unwrap();
        assert_eq!(second_page.artifacts.len(), 1);
        let all = first_page
            .artifacts
            .iter()
            .chain(&second_page.artifacts)
            .map(|artifact| artifact.artifact_id)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            all,
            BTreeSet::from([first.artifact_id, second.artifact_id, third.artifact_id])
        );
    }

    #[tokio::test]
    async fn capability_redemption_is_task_label_byte_and_count_bound() {
        let (service, _) = service();
        let alice = caller("alice", "acme", &["cui"]);
        let task_id = uuid::Uuid::now_v7().to_string();
        let issued = service
            .issue_write_capability(
                &alice,
                IssueArtifactWriteCapabilityRequest {
                    task_id: task_id.clone(),
                    expires_at: Utc::now() + TimeDelta::minutes(10),
                    max_artifact_count: NonZeroU32::new(1).unwrap(),
                    max_total_bytes: NonZeroU64::new(4).unwrap(),
                },
            )
            .await
            .unwrap();
        let request = RedeemArtifactWriteCapabilityRequest {
            capability_id: issued.capability_id,
            task_id: uuid::Uuid::now_v7().to_string(),
            idempotency_key: ArtifactWriteIdempotencyKey::new("media-output-0").unwrap(),
            artifact: PutArtifactRequest::default(),
        };
        assert_eq!(
            service
                .redeem_write_capability(issued.secret.expose_secret(), request, b"data".to_vec())
                .await,
            Err(ArtifactPlaneError::Unauthenticated)
        );

        let request = RedeemArtifactWriteCapabilityRequest {
            capability_id: issued.capability_id,
            task_id,
            idempotency_key: ArtifactWriteIdempotencyKey::new("media-output-0").unwrap(),
            artifact: PutArtifactRequest::default(),
        };
        let metadata = service
            .redeem_write_capability(
                issued.secret.expose_secret(),
                request.clone(),
                b"data".to_vec(),
            )
            .await
            .unwrap();
        assert_eq!(
            metadata.compliance.owner,
            Some(AccessSubject::Principal(alice.identity.actor.id))
        );
        assert_eq!(
            service
                .redeem_write_capability("wrong-secret", request.clone(), b"data".to_vec())
                .await,
            Err(ArtifactPlaneError::Unauthenticated)
        );
        let mut wrong_task = request.clone();
        wrong_task.task_id = uuid::Uuid::now_v7().to_string();
        assert_eq!(
            service
                .redeem_write_capability(
                    issued.secret.expose_secret(),
                    wrong_task,
                    b"data".to_vec(),
                )
                .await,
            Err(ArtifactPlaneError::Unauthenticated)
        );
        let mut wrong_labels = request.clone();
        wrong_labels
            .artifact
            .data_labels
            .insert(DataLabelId::new("restricted").unwrap());
        assert_eq!(
            service
                .redeem_write_capability(
                    issued.secret.expose_secret(),
                    wrong_labels,
                    b"data".to_vec(),
                )
                .await,
            Err(ArtifactPlaneError::Unauthenticated)
        );
        let retry = service
            .redeem_write_capability(issued.secret.expose_secret(), request, b"data".to_vec())
            .await
            .unwrap();
        assert_eq!(retry.artifact_id, metadata.artifact_id);

        let exhausted = RedeemArtifactWriteCapabilityRequest {
            capability_id: issued.capability_id,
            task_id: issued.task_id.clone(),
            idempotency_key: ArtifactWriteIdempotencyKey::new("media-output-1").unwrap(),
            artifact: PutArtifactRequest::default(),
        };
        assert_eq!(
            service
                .redeem_write_capability(
                    issued.secret.expose_secret(),
                    exhausted,
                    b"data".to_vec(),
                )
                .await,
            Err(ArtifactPlaneError::Unauthenticated)
        );
    }

    #[tokio::test]
    async fn staged_capability_write_recovers_after_occurrence_before_finalize() {
        let (service, repository) = service();
        let alice = caller("alice", "acme", &["cui"]);
        let task_id = uuid::Uuid::now_v7().to_string();
        let issued = service
            .issue_write_capability(
                &alice,
                IssueArtifactWriteCapabilityRequest {
                    task_id: task_id.clone(),
                    expires_at: Utc::now() + TimeDelta::minutes(10),
                    max_artifact_count: NonZeroU32::new(1).unwrap(),
                    max_total_bytes: NonZeroU64::new(4).unwrap(),
                },
            )
            .await
            .unwrap();
        let artifact = PutArtifactRequest::default();
        let bytes = b"data".to_vec();
        let sha = compute_sha(&bytes);
        let request_hash = artifact_write_request_hash(&artifact, &sha).unwrap();
        let labels = artifact.effective_labels();
        let reserved = repository
            .reserve_write_capability(WriteCapabilityReservation {
                capability_id: issued.capability_id,
                token_hash: secret_hash(b"veoveo.artifact-write.v1", issued.secret.expose_secret()),
                task_id: task_id.clone(),
                idempotency_key: "media:task:output:0".into(),
                request_hash,
                byte_len: bytes.len() as u64,
                requested_labels: labels,
                proposed_artifact_id: ArtifactId::new(),
            })
            .await
            .unwrap()
            .unwrap();

        // This is the crash boundary: object bytes and occurrence committed,
        // while the redemption remains reserved.
        let staged = service
            .store_occurrence(
                reserved.artifact_id,
                reserved.actor,
                reserved.authority,
                &reserved.labels,
                artifact.clone(),
                bytes.clone(),
            )
            .await
            .unwrap();
        let request = RedeemArtifactWriteCapabilityRequest {
            capability_id: issued.capability_id,
            task_id,
            idempotency_key: ArtifactWriteIdempotencyKey::new("media:task:output:0").unwrap(),
            artifact,
        };
        let recovered = service
            .redeem_write_capability(
                issued.secret.expose_secret(),
                request.clone(),
                b"DIFF".to_vec(),
            )
            .await
            .unwrap();
        assert_eq!(recovered.artifact_id, staged.metadata.artifact_id);
        let response_lost_retry = service
            .redeem_write_capability(issued.secret.expose_secret(), request, b"MORE".to_vec())
            .await
            .unwrap();
        assert_eq!(response_lost_retry.artifact_id, recovered.artifact_id);
    }

    #[tokio::test]
    async fn unstaged_reservation_rebinds_to_nondeterministic_retry() {
        let (service, repository) = service();
        let alice = caller("alice", "acme", &[]);
        let task_id = uuid::Uuid::now_v7().to_string();
        let issued = service
            .issue_write_capability(
                &alice,
                IssueArtifactWriteCapabilityRequest {
                    task_id: task_id.clone(),
                    expires_at: Utc::now() + TimeDelta::minutes(10),
                    max_artifact_count: NonZeroU32::new(1).unwrap(),
                    max_total_bytes: NonZeroU64::new(16).unwrap(),
                },
            )
            .await
            .unwrap();
        let artifact = PutArtifactRequest::default();
        let first_bytes = b"first".to_vec();
        let first_sha = compute_sha(&first_bytes);
        let first_hash = artifact_write_request_hash(&artifact, &first_sha).unwrap();
        repository
            .reserve_write_capability(WriteCapabilityReservation {
                capability_id: issued.capability_id,
                token_hash: secret_hash(b"veoveo.artifact-write.v1", issued.secret.expose_secret()),
                task_id: task_id.clone(),
                idempotency_key: "optimization:task:artifact:0".into(),
                request_hash: first_hash,
                byte_len: first_bytes.len() as u64,
                requested_labels: BTreeSet::new(),
                proposed_artifact_id: ArtifactId::new(),
            })
            .await
            .unwrap()
            .unwrap();

        // No occurrence was staged before the process died. A regenerated
        // byte stream may differ, so the pending reservation is rebound and
        // the one reserved occurrence is completed with the retry bytes.
        let retry_bytes = b"second-version".to_vec();
        let completed = service
            .redeem_write_capability(
                issued.secret.expose_secret(),
                RedeemArtifactWriteCapabilityRequest {
                    capability_id: issued.capability_id,
                    task_id,
                    idempotency_key: ArtifactWriteIdempotencyKey::new(
                        "optimization:task:artifact:0",
                    )
                    .unwrap(),
                    artifact,
                },
                retry_bytes.clone(),
            )
            .await
            .unwrap();
        assert_eq!(completed.byte_len, retry_bytes.len() as u64);
        assert_eq!(
            service
                .store
                .get_bounded(
                    &repository
                        .get_artifact(completed.artifact_id)
                        .await
                        .unwrap()
                        .unwrap()
                        .object_key,
                    64,
                )
                .await
                .unwrap(),
            retry_bytes
        );
    }

    #[tokio::test]
    async fn public_links_require_release_and_enforce_atomic_download_limits_and_revocation() {
        let (service, _) = service();
        let alice = caller("alice", "acme", &[]);
        let metadata = service
            .put(
                &alice,
                PutArtifactRequest::default(),
                b"public-data".to_vec(),
            )
            .await
            .unwrap();
        assert!(matches!(
            service
                .create_share_link(
                    &alice,
                    &metadata.artifact_id,
                    CreateArtifactShareLinkRequest::default(),
                )
                .await,
            Err(ArtifactPlaneError::Conflict(_))
        ));
        service
            .set_release_state(
                &alice,
                &metadata.artifact_id,
                ArtifactReleaseState::Releasable,
            )
            .await
            .unwrap();
        let link = service
            .create_share_link(
                &alice,
                &metadata.artifact_id,
                CreateArtifactShareLinkRequest {
                    expires_at: None,
                    max_downloads: NonZeroU64::new(1),
                },
            )
            .await
            .unwrap();
        let token = link.url.rsplit('/').next().unwrap();
        let first = service.redeem_public_share(token).await.unwrap();
        assert_eq!(first.metadata.artifact_id, metadata.artifact_id);
        assert!(matches!(
            service.redeem_public_share(token).await,
            Err(ArtifactPlaneError::NotFound)
        ));

        let revocable = service
            .create_share_link(
                &alice,
                &metadata.artifact_id,
                CreateArtifactShareLinkRequest::default(),
            )
            .await
            .unwrap();
        let token = revocable.url.rsplit('/').next().unwrap().to_owned();
        service
            .revoke_share_link(&alice, &metadata.artifact_id, &revocable.link_id)
            .await
            .unwrap();
        assert!(matches!(
            service.redeem_public_share(&token).await,
            Err(ArtifactPlaneError::NotFound)
        ));
    }

    #[tokio::test]
    async fn owner_admin_grant_cannot_be_lowered_or_revoked() {
        let (service, _) = service();
        let alice = caller("alice", "acme", &[]);
        let artifact = service
            .put(&alice, PutArtifactRequest::default(), b"owned".to_vec())
            .await
            .unwrap();
        let owner = AccessSubject::Principal(alice.identity.actor.id.clone());
        assert!(matches!(
            service
                .grant(
                    &alice,
                    &artifact.artifact_id,
                    owner.clone(),
                    AccessLevel::Read
                )
                .await,
            Err(ArtifactPlaneError::Conflict(_))
        ));
        assert!(matches!(
            service.revoke(&alice, &artifact.artifact_id, &owner).await,
            Err(ArtifactPlaneError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn access_request_approval_adds_a_grant_without_bypassing_clearance() {
        let (service, _) = service();
        let alice = caller("alice", "acme", &["controlled"]);
        let mut bob = caller("bob", "acme", &["controlled"]);
        bob.identity.authority.work_context = WorkContextId::new("other-work").unwrap();
        bob.identity.authority.membership = WorkContextMembershipLevel::Viewer;
        let mut custodian = caller("casey", "acme", &["controlled"]);
        custodian.identity.authority.membership = WorkContextMembershipLevel::Custodian;

        let artifact = service
            .put(
                &alice,
                PutArtifactRequest {
                    classification: Some(DataLabelId::new("controlled").unwrap()),
                    ..PutArtifactRequest::default()
                },
                b"governed".to_vec(),
            )
            .await
            .unwrap();
        assert_eq!(
            service
                .get(&bob, &artifact.artifact_id, AccessLevel::Read)
                .await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyNeedToKnow))
        );

        let requested = service
            .create_access_request(
                &bob,
                &artifact.artifact_id,
                CreateArtifactAccessRequest {
                    requested_level: AccessLevel::Read,
                    justification: "Required for the assigned review.".into(),
                },
            )
            .await
            .unwrap();
        let mine = service
            .list_access_requests(
                &bob,
                ListArtifactAccessRequests {
                    scope: Some(ArtifactAccessRequestScope::Mine),
                    state: Some(ArtifactAccessRequestState::Pending),
                    ..ListArtifactAccessRequests::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(mine.requests, vec![requested.clone()]);

        let queue = service
            .list_access_requests(
                &custodian,
                ListArtifactAccessRequests {
                    scope: Some(ArtifactAccessRequestScope::Reviewable),
                    state: Some(ArtifactAccessRequestState::Pending),
                    ..ListArtifactAccessRequests::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(queue.requests, vec![requested.clone()]);
        let approved = service
            .decide_access_request(
                &custodian,
                &requested.id,
                DecideArtifactAccessRequest {
                    decision: ArtifactAccessRequestDecision::Approve,
                    note: Some("Review assignment verified.".into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(approved.state, ArtifactAccessRequestState::Approved);
        assert_eq!(
            service
                .get(&bob, &artifact.artifact_id, AccessLevel::Read)
                .await
                .unwrap()
                .bytes,
            b"governed"
        );

        let mut no_clearance = caller("dana", "acme", &[]);
        no_clearance.identity.authority.work_context = WorkContextId::new("other-work").unwrap();
        assert_eq!(
            service
                .create_access_request(
                    &no_clearance,
                    &artifact.artifact_id,
                    CreateArtifactAccessRequest {
                        requested_level: AccessLevel::Read,
                        justification: "Requesting review access.".into(),
                    },
                )
                .await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyClearance))
        );
    }
}
