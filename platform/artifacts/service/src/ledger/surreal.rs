//! SurrealDB implementation of the authoritative artifact repository.

use std::collections::{BTreeMap, BTreeSet};

use veoveo_mcp_contract::access::{AccessLevel, ArtifactId, Grant};
use veoveo_mcp_contract::gateway::{
    DataLabelId, DelegationId, GatewayProfileId, GroupId, PolicyVersion, PrincipalId,
    PrincipalKind, ServerSlug, TenantId, TokenIssuer, TokenSubject, WorkContextId,
};
use veoveo_mcp_contract::storage::{
    ArtifactMetadata, ArtifactProvenance, ArtifactReleaseState, ComplianceMetadata,
};
use veoveo_mcp_contract::{
    AccessSubject, ArtifactAccessRequest, ArtifactAccessRequestDecision, ArtifactAccessRequestId,
    ArtifactAccessRequestState, ArtifactShareLinkId, InvocationAuthority, InvocationProvenance,
    WorkContextGrant, WorkContextMembershipLevel, WorkContextOutputPolicy,
};
use veoveo_platform_store as platform;
use veoveo_platform_store::{RecordIdKey, StoreError as PlatformStoreError};

use super::{
    ArtifactAccessRequestCancellation, ArtifactAccessRequestDecisionDraft,
    ArtifactAccessRequestListQuery, ArtifactAuditEvent, ArtifactListQuery, ArtifactRepository,
    AuditOutcome, BlobSha256, NewArtifact, NewArtifactAccessRequest, RedeemedWriteCapability,
    RepositoryActor, RepositoryError, ShareLinkDraft, StoredArtifact, WriteCapabilityDraft,
    WriteCapabilityReservation,
};

#[derive(Clone, Debug)]
pub struct SurrealArtifactRepository {
    store: platform::PlatformStore,
}

impl SurrealArtifactRepository {
    pub fn new(store: platform::PlatformStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &platform::PlatformStore {
        &self.store
    }

    async fn identity(
        &self,
        actor: &RepositoryActor,
    ) -> Result<platform::PlatformIdentity, RepositoryError> {
        self.store
            .ensure_identity(
                actor.tenant.as_str(),
                actor.principal.as_str(),
                actor.issuer.as_str(),
                actor.subject.as_str(),
                principal_kind(actor.kind),
            )
            .await
            .map_err(repository_error)
    }

    async fn subject_record(
        &self,
        identity: &platform::PlatformIdentity,
        subject: &AccessSubject,
    ) -> Result<platform::RecordId, RepositoryError> {
        match subject {
            AccessSubject::Principal(principal) if principal.as_str() == identity.principal_key => {
                Ok(identity.principal_id.record_id())
            }
            AccessSubject::Principal(principal) => Ok(platform::deterministic_principal_id(
                &identity.tenant_key,
                principal.as_str(),
            )
            .map_err(repository_error)?
            .record_id()),
            AccessSubject::Group(group) => Ok(self
                .store
                .ensure_group(identity, group.as_str())
                .await
                .map_err(repository_error)?
                .record_id()),
        }
    }

    async fn map_aggregate(
        &self,
        artifact_id: ArtifactId,
        aggregate: platform::ArtifactAggregate,
    ) -> Result<StoredArtifact, RepositoryError> {
        let tenant = TenantId::new(aggregate.tenant.slug)
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?;
        let authority = contract_authority(tenant.clone(), aggregate.occurrence.authority.clone())?;
        let labels = parse_labels(aggregate.occurrence.labels)?;
        let classification = if aggregate.occurrence.classification.is_empty() {
            None
        } else {
            Some(
                DataLabelId::new(aggregate.occurrence.classification)
                    .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
            )
        };
        let mut data_labels = labels.clone();
        if let Some(classification) = &classification {
            data_labels.remove(classification);
        }
        let retention_expires_at = aggregate.occurrence.retention_expires_at;
        let grants = aggregate
            .grants
            .into_iter()
            .map(|edge| {
                let subject = match edge.subject_kind {
                    platform::ArtifactGrantSubjectKind::Principal => AccessSubject::Principal(
                        PrincipalId::new(edge.subject_key)
                            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
                    ),
                    platform::ArtifactGrantSubjectKind::Group => AccessSubject::Group(
                        GroupId::new(edge.subject_key)
                            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
                    ),
                };
                Ok(Grant {
                    artifact: artifact_id,
                    subject,
                    level: access_level(edge.permission),
                    tenant: tenant.clone(),
                    data_labels: parse_labels(edge.labels)?,
                    retention_expires_at: edge.expires_at,
                })
            })
            .collect::<Result<Vec<_>, RepositoryError>>()?;
        let metadata = ArtifactMetadata {
            artifact_id,
            byte_len: u64::try_from(aggregate.blob.byte_len)
                .map_err(|_| RepositoryError::Corrupt("negative artifact byte length".into()))?,
            mime_type: (!aggregate.occurrence.media_type.is_empty())
                .then_some(aggregate.occurrence.media_type),
            filename: aggregate.occurrence.filename,
            artifact_uri: artifact_id.plane_uri(),
            download_url: None,
            created_at: aggregate.occurrence.created_at,
            release_state: release_state(aggregate.occurrence.release_state),
            compliance: ComplianceMetadata {
                classification,
                tenant_id: Some(tenant.clone()),
                owner: Some(authority.output_policy.owner.clone()),
                work_context: Some(authority.work_context.clone()),
                provenance: Some(ArtifactProvenance {
                    producer: PrincipalId::new(aggregate.occurrence.producer_key)
                        .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
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
                data_labels,
                retention_expires_at,
            },
            metadata: serde_json::Value::Object(
                aggregate
                    .occurrence
                    .metadata
                    .into_map()
                    .into_iter()
                    .collect(),
            ),
        };
        Ok(StoredArtifact {
            metadata,
            tenant,
            labels,
            grants,
            authority,
            blob_sha256: BlobSha256::new(aggregate.blob.sha256)
                .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
            object_key: aggregate.blob.object_key,
        })
    }
}

impl ArtifactRepository for SurrealArtifactRepository {
    async fn create_artifact(
        &self,
        artifact: NewArtifact,
    ) -> Result<StoredArtifact, RepositoryError> {
        let identity = self.identity(&artifact.actor).await?;
        if artifact.stored.authority.tenant != artifact.actor.tenant {
            return Err(RepositoryError::Corrupt(
                "artifact authority tenant differs from the producing actor".into(),
            ));
        }
        let authority = platform_authority(&artifact.stored.authority);
        let owner = self
            .subject_record(&identity, &artifact.stored.authority.output_policy.owner)
            .await?;
        let mut initial_grants = Vec::with_capacity(artifact.stored.grants.len());
        for grant in &artifact.stored.grants {
            if grant.artifact != artifact.stored.metadata.artifact_id
                || grant.tenant != artifact.actor.tenant
            {
                return Err(RepositoryError::Corrupt(
                    "artifact initial grant is outside its occurrence boundary".into(),
                ));
            }
            initial_grants.push(platform::ArtifactGrantDraft {
                artifact_id: platform::ArtifactId::from_uuid(grant.artifact.as_uuid()),
                subject: self.subject_record(&identity, &grant.subject).await?,
                subject_kind: platform_subject_kind(&grant.subject),
                subject_key: subject_key(&grant.subject).to_owned(),
                permission: grant_permission(grant.level),
                labels: labels_to_strings(&grant.data_labels),
                expires_at: grant.retention_expires_at,
                created_by: identity.principal_id,
            });
        }
        let metadata = match artifact.stored.metadata.metadata.clone() {
            serde_json::Value::Null => BTreeMap::new(),
            serde_json::Value::Object(values) => values.into_iter().collect(),
            _ => {
                return Err(RepositoryError::Corrupt(
                    "artifact metadata must be an object or null".into(),
                ));
            }
        };
        let draft = platform::ArtifactOccurrenceDraft {
            artifact_id: platform::ArtifactId::from_uuid(
                artifact.stored.metadata.artifact_id.as_uuid(),
            ),
            identity,
            authority,
            owner,
            initial_grants,
            sha256: artifact.stored.blob_sha256.as_str().to_owned(),
            byte_len: i64::try_from(artifact.stored.metadata.byte_len)
                .map_err(|_| RepositoryError::Corrupt("artifact is too large".into()))?,
            object_key: artifact.stored.object_key,
            media_type: artifact
                .stored
                .metadata
                .mime_type
                .clone()
                .unwrap_or_default(),
            filename: artifact.stored.metadata.filename.clone(),
            classification: artifact
                .stored
                .metadata
                .compliance
                .classification
                .as_ref()
                .map_or_else(String::new, |label| label.as_str().to_owned()),
            labels: labels_to_strings(&artifact.stored.labels),
            metadata,
            retention_expires_at: artifact.stored.metadata.compliance.retention_expires_at,
        };
        let aggregate = self
            .store
            .create_artifact_occurrence(draft)
            .await
            .map_err(repository_error)?;
        self.map_aggregate(artifact.stored.metadata.artifact_id, aggregate)
            .await
    }

    async fn get_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<Option<StoredArtifact>, RepositoryError> {
        let aggregate = self
            .store
            .artifact_aggregate(platform::ArtifactId::from_uuid(artifact_id.as_uuid()))
            .await
            .map_err(repository_error)?;
        match aggregate {
            Some(aggregate) => self.map_aggregate(artifact_id, aggregate).await.map(Some),
            None => Ok(None),
        }
    }

    async fn list_artifacts(
        &self,
        query: ArtifactListQuery,
    ) -> Result<Vec<StoredArtifact>, RepositoryError> {
        let identity = self.identity(&query.actor).await?;
        let mut subjects = vec![identity.principal_id.record_id()];
        for group in &query.groups {
            subjects.push(
                platform::deterministic_group_id(&identity.tenant_key, group.as_str())
                    .map_err(repository_error)?
                    .record_id(),
            );
        }
        let ids = self
            .store
            .artifact_ids_for_subjects(
                identity.tenant_id,
                subjects,
                query
                    .cursor
                    .map(|id| platform::ArtifactId::from_uuid(id.as_uuid())),
                query.limit,
            )
            .await
            .map_err(repository_error)?;
        let mut artifacts = Vec::with_capacity(ids.len());
        for id in ids {
            let contract_id = ArtifactId::parse(id.to_string())
                .map_err(|error| RepositoryError::Corrupt(error.to_string()))?;
            let aggregate = self
                .store
                .artifact_aggregate(id)
                .await
                .map_err(repository_error)?
                .ok_or_else(|| {
                    RepositoryError::Corrupt(
                        "artifact disappeared while building discovery page".into(),
                    )
                })?;
            artifacts.push(self.map_aggregate(contract_id, aggregate).await?);
        }
        Ok(artifacts)
    }

    async fn upsert_grant(
        &self,
        actor: &RepositoryActor,
        grant: Grant,
    ) -> Result<(), RepositoryError> {
        let creator = self.identity(actor).await?;
        let (subject_record, subject_kind, subject_key) = match &grant.subject {
            AccessSubject::Principal(user) => {
                let target =
                    platform::deterministic_principal_id(grant.tenant.as_str(), user.as_str())
                        .map_err(repository_error)?;
                (
                    target.record_id(),
                    platform::ArtifactGrantSubjectKind::Principal,
                    user.as_str().to_owned(),
                )
            }
            AccessSubject::Group(group) => {
                let group_id = self
                    .store
                    .ensure_group(&creator, group.as_str())
                    .await
                    .map_err(repository_error)?;
                (
                    group_id.record_id(),
                    platform::ArtifactGrantSubjectKind::Group,
                    group.as_str().to_owned(),
                )
            }
        };
        self.store
            .upsert_artifact_grant(platform::ArtifactGrantDraft {
                artifact_id: platform::ArtifactId::from_uuid(grant.artifact.as_uuid()),
                subject: subject_record,
                subject_kind,
                subject_key,
                permission: grant_permission(grant.level),
                labels: labels_to_strings(&grant.data_labels),
                expires_at: grant.retention_expires_at,
                created_by: creator.principal_id,
            })
            .await
            .map_err(repository_error)
    }

    async fn remove_grant(
        &self,
        artifact_id: ArtifactId,
        subject: &AccessSubject,
    ) -> Result<(), RepositoryError> {
        let (kind, key) = match subject {
            AccessSubject::Principal(id) => {
                (platform::ArtifactGrantSubjectKind::Principal, id.as_str())
            }
            AccessSubject::Group(id) => (platform::ArtifactGrantSubjectKind::Group, id.as_str()),
        };
        self.store
            .remove_artifact_grant(
                platform::ArtifactId::from_uuid(artifact_id.as_uuid()),
                kind,
                key,
            )
            .await
            .map_err(repository_error)
    }

    async fn set_release_state(
        &self,
        artifact_id: ArtifactId,
        state: ArtifactReleaseState,
    ) -> Result<Option<StoredArtifact>, RepositoryError> {
        let updated = self
            .store
            .set_artifact_release_state(
                platform::ArtifactId::from_uuid(artifact_id.as_uuid()),
                platform_release_state(state),
            )
            .await
            .map_err(repository_error)?;
        if updated.is_none() {
            return Ok(None);
        }
        self.get_artifact(artifact_id).await
    }

    async fn create_write_capability(
        &self,
        draft: WriteCapabilityDraft,
    ) -> Result<(), RepositoryError> {
        let identity = self.identity(&draft.actor).await?;
        self.store
            .create_artifact_write_capability(platform::ArtifactWriteCapabilityDraft {
                capability_id: platform::ArtifactWriteCapabilityId::from_uuid(
                    draft.capability_id.as_uuid(),
                ),
                identity,
                authority: platform_authority(&draft.authority),
                profile_key: draft.profile.as_str().to_owned(),
                server_key: draft.server.as_str().to_owned(),
                task_id: draft.task_id,
                actor_kind: principal_kind(draft.actor.kind),
                actor_issuer: draft.actor.issuer.as_str().to_owned(),
                actor_subject: draft.actor.subject.as_str().to_owned(),
                token_hash: draft.token_hash,
                labels: labels_to_strings(&draft.labels),
                max_artifact_count: i64::from(draft.max_artifact_count),
                max_total_bytes: i64::try_from(draft.max_total_bytes).map_err(|_| {
                    RepositoryError::Corrupt("capability byte limit is too large".into())
                })?,
                expires_at: draft.expires_at,
            })
            .await
            .map(|_| ())
            .map_err(repository_error)
    }

    async fn reserve_write_capability(
        &self,
        request: WriteCapabilityReservation,
    ) -> Result<Option<RedeemedWriteCapability>, RepositoryError> {
        let redemption = self
            .store
            .reserve_artifact_write_capability(
                platform::ArtifactWriteCapabilityId::from_uuid(request.capability_id.as_uuid()),
                &request.token_hash,
                &request.task_id,
                &request.idempotency_key,
                &request.request_hash,
                i64::try_from(request.byte_len)
                    .map_err(|_| RepositoryError::Corrupt("artifact is too large".into()))?,
                &labels_to_strings(&request.requested_labels),
                platform::ArtifactId::from_uuid(request.proposed_artifact_id.as_uuid()),
            )
            .await;
        let redemption = match redemption {
            Ok(redemption) => redemption,
            Err(PlatformStoreError::ArtifactWriteDenied) => return Ok(None),
            Err(error) => return Err(repository_error(error)),
        };
        let capability = redemption.capability;
        let tenant = TenantId::new(capability.tenant_key.clone())
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?;
        let authority = contract_authority(tenant.clone(), capability.authority)?;
        Ok(Some(RedeemedWriteCapability {
            redemption_id: record_uuid(&redemption.redemption.id)?,
            artifact_id: ArtifactId::parse(
                record_uuid(&redemption.redemption.artifact)?.to_string(),
            )
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
            actor: RepositoryActor {
                tenant,
                principal: PrincipalId::new(capability.actor_key)
                    .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
                kind: contract_principal_kind(capability.actor_kind),
                issuer: TokenIssuer::new(capability.actor_issuer)
                    .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
                subject: TokenSubject::new(capability.actor_subject)
                    .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
            },
            authority,
            labels: parse_labels(capability.labels)?,
            profile: GatewayProfileId::new(capability.profile_key)
                .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
            server: ServerSlug::new(capability.server_key)
                .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
            task_id: capability.task_id,
            finalized: redemption.redemption.state
                == platform::ArtifactWriteRedemptionState::Finalized,
            request_matches: redemption.request_matches,
        }))
    }

    async fn finalize_write_capability(
        &self,
        redemption_id: uuid::Uuid,
        artifact_id: ArtifactId,
    ) -> Result<bool, RepositoryError> {
        self.store
            .finalize_artifact_write_capability(
                platform::ArtifactWriteRedemptionId::from_uuid(redemption_id),
                platform::ArtifactId::from_uuid(artifact_id.as_uuid()),
            )
            .await
            .map_err(repository_error)
    }

    async fn create_share_link(&self, draft: ShareLinkDraft) -> Result<(), RepositoryError> {
        let identity = self.identity(&draft.actor).await?;
        self.store
            .create_artifact_share_link(platform::ArtifactShareLinkDraft {
                link_id: platform::ShareLinkId::from_uuid(draft.link_id.as_uuid()),
                artifact_id: platform::ArtifactId::from_uuid(draft.artifact_id.as_uuid()),
                identity,
                token_hash: draft.token_hash,
                expires_at: draft.expires_at,
                max_downloads: draft.max_downloads.map(i64::try_from).transpose().map_err(
                    |_| RepositoryError::Corrupt("share download limit is too large".into()),
                )?,
            })
            .await
            .map(|_| ())
            .map_err(repository_error)
    }

    async fn revoke_share_link(
        &self,
        artifact_id: ArtifactId,
        link_id: ArtifactShareLinkId,
    ) -> Result<bool, RepositoryError> {
        self.store
            .revoke_artifact_share_link(
                platform::ShareLinkId::from_uuid(link_id.as_uuid()),
                platform::ArtifactId::from_uuid(artifact_id.as_uuid()),
            )
            .await
            .map_err(repository_error)
    }

    async fn redeem_share_link(
        &self,
        token_hash: &str,
    ) -> Result<Option<ArtifactId>, RepositoryError> {
        self.store
            .redeem_public_share_link(token_hash)
            .await
            .map_err(repository_error)?
            .map(|redemption| {
                ArtifactId::parse(redemption.artifact_id.to_string())
                    .map_err(|error| RepositoryError::Corrupt(error.to_string()))
            })
            .transpose()
    }

    async fn append_audit(&self, event: ArtifactAuditEvent) -> Result<(), RepositoryError> {
        let identity = match &event.actor {
            Some(actor) => Some(self.identity(actor).await?),
            None => match &event.tenant {
                Some(tenant) => Some(
                    self.store
                        .ensure_identity(
                            tenant.as_str(),
                            "artifact-public-reader",
                            "veoveo-artifact-service",
                            "public-share",
                            platform::PrincipalKind::Service,
                        )
                        .await
                        .map_err(repository_error)?,
                ),
                None => None,
            },
        };
        self.store
            .append_artifact_audit(platform::ArtifactAuditDraft {
                tenant: identity.as_ref().map(|identity| identity.tenant_id),
                actor: event
                    .actor
                    .as_ref()
                    .and_then(|_| identity.as_ref().map(|identity| identity.principal_id)),
                action: event.action,
                resource_id: event.artifact_id.map(|id| id.to_string()),
                outcome: match event.outcome {
                    AuditOutcome::Allowed => platform::AuditOutcome::Allowed,
                    AuditOutcome::Denied => platform::AuditOutcome::Denied,
                    AuditOutcome::Failed => platform::AuditOutcome::Failed,
                },
                details: event.details.into_iter().collect(),
            })
            .await
            .map_err(repository_error)
    }

    async fn create_or_reopen_access_request(
        &self,
        request: NewArtifactAccessRequest,
    ) -> Result<ArtifactAccessRequest, RepositoryError> {
        let identity = self.identity(&request.actor).await?;
        let record = self
            .store
            .create_or_reopen_artifact_access_request(platform::ArtifactAccessRequestDraft {
                request_id: platform::ArtifactAccessRequestId::from_uuid(
                    request.request_id.as_uuid(),
                ),
                identity,
                artifact_id: platform::ArtifactId::from_uuid(request.artifact_id.as_uuid()),
                requested_level: grant_permission(request.requested_level),
                justification: request.justification,
            })
            .await
            .map_err(repository_error)?;
        contract_access_request(record)
    }

    async fn get_access_request(
        &self,
        actor: &RepositoryActor,
        request_id: ArtifactAccessRequestId,
    ) -> Result<Option<ArtifactAccessRequest>, RepositoryError> {
        let identity = self.identity(actor).await?;
        self.store
            .artifact_access_request(
                identity.tenant_id,
                platform::ArtifactAccessRequestId::from_uuid(request_id.as_uuid()),
            )
            .await
            .map_err(repository_error)?
            .map(contract_access_request)
            .transpose()
    }

    async fn list_access_requests(
        &self,
        query: ArtifactAccessRequestListQuery,
    ) -> Result<Vec<ArtifactAccessRequest>, RepositoryError> {
        let identity = self.identity(&query.actor).await?;
        let work_context_id = query
            .work_context
            .as_ref()
            .map(|context| {
                platform::deterministic_work_context_id(
                    query.actor.tenant.as_str(),
                    context.as_str(),
                )
            })
            .transpose()
            .map_err(repository_error)?;
        self.store
            .list_artifact_access_requests(platform::ArtifactAccessRequestQuery {
                tenant_id: identity.tenant_id,
                requester_id: work_context_id.is_none().then_some(identity.principal_id),
                work_context_id,
                state: query.state.map(platform_access_request_state),
                cursor: query
                    .cursor
                    .map(|cursor| platform::ArtifactAccessRequestId::from_uuid(cursor.as_uuid())),
                limit: u32::try_from(query.limit).map_err(|_| {
                    RepositoryError::Corrupt("access request limit is too large".into())
                })?,
            })
            .await
            .map_err(repository_error)?
            .into_iter()
            .map(contract_access_request)
            .collect()
    }

    async fn decide_access_request(
        &self,
        decision: ArtifactAccessRequestDecisionDraft,
    ) -> Result<ArtifactAccessRequest, RepositoryError> {
        let identity = self.identity(&decision.actor).await?;
        let state = match decision.decision {
            ArtifactAccessRequestDecision::Approve => {
                platform::ArtifactAccessRequestState::Approved
            }
            ArtifactAccessRequestDecision::Deny => platform::ArtifactAccessRequestState::Denied,
        };
        self.store
            .decide_artifact_access_request(platform::ArtifactAccessRequestDecisionDraft {
                identity,
                request_id: platform::ArtifactAccessRequestId::from_uuid(
                    decision.request_id.as_uuid(),
                ),
                state,
                note: decision.note,
            })
            .await
            .map_err(repository_error)
            .and_then(contract_access_request)
    }

    async fn cancel_access_request(
        &self,
        cancellation: ArtifactAccessRequestCancellation,
    ) -> Result<ArtifactAccessRequest, RepositoryError> {
        let identity = self.identity(&cancellation.actor).await?;
        let existing = self
            .store
            .artifact_access_request(
                identity.tenant_id,
                platform::ArtifactAccessRequestId::from_uuid(cancellation.request_id.as_uuid()),
            )
            .await
            .map_err(repository_error)?
            .ok_or_else(|| RepositoryError::Conflict("access request does not exist".into()))?;
        if existing.requester != identity.principal_id.record_id() {
            return Err(RepositoryError::Conflict(
                "access request belongs to another principal".into(),
            ));
        }
        self.store
            .decide_artifact_access_request(platform::ArtifactAccessRequestDecisionDraft {
                identity,
                request_id: platform::ArtifactAccessRequestId::from_uuid(
                    cancellation.request_id.as_uuid(),
                ),
                state: platform::ArtifactAccessRequestState::Cancelled,
                note: None,
            })
            .await
            .map_err(repository_error)
            .and_then(contract_access_request)
    }
}

fn contract_access_request(
    record: platform::ArtifactAccessRequestRecord,
) -> Result<ArtifactAccessRequest, RepositoryError> {
    Ok(ArtifactAccessRequest {
        id: ArtifactAccessRequestId::parse(record_uuid(&record.id)?.to_string())
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
        artifact_id: ArtifactId::parse(record_uuid(&record.artifact)?.to_string())
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
        work_context: WorkContextId::new(record.work_context_key)
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
        requester: PrincipalId::new(record.requester_key)
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
        requested_level: access_level(record.requested_level),
        justification: record.justification,
        state: contract_access_request_state(record.state),
        decided_by: record
            .decided_by_key
            .map(PrincipalId::new)
            .transpose()
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
        decision_note: record.decision_note,
        created_at: record.created_at,
        updated_at: record.updated_at,
        decided_at: record.decided_at,
    })
}

fn platform_access_request_state(
    state: ArtifactAccessRequestState,
) -> platform::ArtifactAccessRequestState {
    match state {
        ArtifactAccessRequestState::Pending => platform::ArtifactAccessRequestState::Pending,
        ArtifactAccessRequestState::Approved => platform::ArtifactAccessRequestState::Approved,
        ArtifactAccessRequestState::Denied => platform::ArtifactAccessRequestState::Denied,
        ArtifactAccessRequestState::Cancelled => platform::ArtifactAccessRequestState::Cancelled,
    }
}

fn contract_access_request_state(
    state: platform::ArtifactAccessRequestState,
) -> ArtifactAccessRequestState {
    match state {
        platform::ArtifactAccessRequestState::Pending => ArtifactAccessRequestState::Pending,
        platform::ArtifactAccessRequestState::Approved => ArtifactAccessRequestState::Approved,
        platform::ArtifactAccessRequestState::Denied => ArtifactAccessRequestState::Denied,
        platform::ArtifactAccessRequestState::Cancelled => ArtifactAccessRequestState::Cancelled,
    }
}

fn platform_authority(authority: &InvocationAuthority) -> platform::InvocationAuthorityRecord {
    let (invocation_mode, initiator_key, delegation_id) = match &authority.provenance {
        InvocationProvenance::Direct { initiator } => (
            platform::InvocationMode::Direct,
            Some(initiator.as_str().to_owned()),
            None,
        ),
        InvocationProvenance::Delegated {
            initiator,
            delegation_id,
        } => (
            platform::InvocationMode::Delegated,
            Some(initiator.as_str().to_owned()),
            Some(delegation_id.as_str().to_owned()),
        ),
        InvocationProvenance::Automated => (platform::InvocationMode::Automated, None, None),
    };
    platform::InvocationAuthorityRecord {
        context_key: authority.work_context.as_str().to_owned(),
        membership: platform_membership(authority.membership),
        policy_revision: authority.policy_revision.as_str().to_owned(),
        owner_kind: platform_subject_kind(&authority.output_policy.owner),
        owner_key: subject_key(&authority.output_policy.owner).to_owned(),
        initial_grants: authority
            .output_policy
            .initial_grants
            .iter()
            .map(|grant| platform::WorkContextInitialGrantRecord {
                subject_kind: platform_subject_kind(&grant.subject),
                subject_key: subject_key(&grant.subject).to_owned(),
                permission: grant_permission(grant.level),
            })
            .collect(),
        classification: authority
            .output_policy
            .classification
            .as_ref()
            .map(|classification| classification.as_str().to_owned()),
        data_labels: labels_to_strings(&authority.output_policy.data_labels),
        invocation_mode,
        initiator_key,
        delegation_id,
    }
}

fn contract_authority(
    tenant: TenantId,
    authority: platform::InvocationAuthorityRecord,
) -> Result<InvocationAuthority, RepositoryError> {
    let provenance = match authority.invocation_mode {
        platform::InvocationMode::Direct => {
            if authority.delegation_id.is_some() {
                return Err(RepositoryError::Corrupt(
                    "direct artifact authority carries a delegation identity".into(),
                ));
            }
            InvocationProvenance::Direct {
                initiator: parse_principal(authority.initiator_key, "direct initiator")?,
            }
        }
        platform::InvocationMode::Delegated => InvocationProvenance::Delegated {
            initiator: parse_principal(authority.initiator_key, "delegated initiator")?,
            delegation_id: DelegationId::new(authority.delegation_id.ok_or_else(|| {
                RepositoryError::Corrupt(
                    "delegated artifact authority is missing its delegation identity".into(),
                )
            })?)
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
        },
        platform::InvocationMode::Automated => {
            if authority.initiator_key.is_some() || authority.delegation_id.is_some() {
                return Err(RepositoryError::Corrupt(
                    "automated artifact authority carries interactive provenance".into(),
                ));
            }
            InvocationProvenance::Automated
        }
    };
    let owner = contract_subject(authority.owner_kind, authority.owner_key)?;
    let initial_grants = authority
        .initial_grants
        .into_iter()
        .map(|grant| {
            Ok(WorkContextGrant {
                subject: contract_subject(grant.subject_kind, grant.subject_key)?,
                level: access_level(grant.permission),
            })
        })
        .collect::<Result<Vec<_>, RepositoryError>>()?;
    let classification = authority
        .classification
        .map(DataLabelId::new)
        .transpose()
        .map_err(|error| RepositoryError::Corrupt(error.to_string()))?;
    Ok(InvocationAuthority {
        work_context: WorkContextId::new(authority.context_key)
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
        tenant,
        membership: contract_membership(authority.membership),
        policy_revision: PolicyVersion::new(authority.policy_revision)
            .map_err(|error| RepositoryError::Corrupt(error.to_string()))?,
        output_policy: WorkContextOutputPolicy {
            owner,
            initial_grants,
            classification,
            data_labels: parse_labels(authority.data_labels)?,
        },
        provenance,
    })
}

fn platform_subject_kind(subject: &AccessSubject) -> platform::ArtifactGrantSubjectKind {
    match subject {
        AccessSubject::Principal(_) => platform::ArtifactGrantSubjectKind::Principal,
        AccessSubject::Group(_) => platform::ArtifactGrantSubjectKind::Group,
    }
}

fn subject_key(subject: &AccessSubject) -> &str {
    match subject {
        AccessSubject::Principal(principal) => principal.as_str(),
        AccessSubject::Group(group) => group.as_str(),
    }
}

fn contract_subject(
    kind: platform::ArtifactGrantSubjectKind,
    key: String,
) -> Result<AccessSubject, RepositoryError> {
    match kind {
        platform::ArtifactGrantSubjectKind::Principal => PrincipalId::new(key)
            .map(AccessSubject::Principal)
            .map_err(|error| RepositoryError::Corrupt(error.to_string())),
        platform::ArtifactGrantSubjectKind::Group => GroupId::new(key)
            .map(AccessSubject::Group)
            .map_err(|error| RepositoryError::Corrupt(error.to_string())),
    }
}

fn parse_principal(
    principal: Option<String>,
    field: &'static str,
) -> Result<PrincipalId, RepositoryError> {
    PrincipalId::new(principal.ok_or_else(|| {
        RepositoryError::Corrupt(format!("artifact authority is missing its {field}"))
    })?)
    .map_err(|error| RepositoryError::Corrupt(error.to_string()))
}

fn platform_membership(
    membership: WorkContextMembershipLevel,
) -> platform::WorkContextMembershipLevel {
    match membership {
        WorkContextMembershipLevel::Viewer => platform::WorkContextMembershipLevel::Viewer,
        WorkContextMembershipLevel::Contributor => {
            platform::WorkContextMembershipLevel::Contributor
        }
        WorkContextMembershipLevel::Custodian => platform::WorkContextMembershipLevel::Custodian,
        WorkContextMembershipLevel::Owner => platform::WorkContextMembershipLevel::Owner,
    }
}

fn contract_membership(
    membership: platform::WorkContextMembershipLevel,
) -> WorkContextMembershipLevel {
    match membership {
        platform::WorkContextMembershipLevel::Viewer => WorkContextMembershipLevel::Viewer,
        platform::WorkContextMembershipLevel::Contributor => {
            WorkContextMembershipLevel::Contributor
        }
        platform::WorkContextMembershipLevel::Custodian => WorkContextMembershipLevel::Custodian,
        platform::WorkContextMembershipLevel::Owner => WorkContextMembershipLevel::Owner,
    }
}

fn labels_to_strings(labels: &BTreeSet<DataLabelId>) -> Vec<String> {
    labels
        .iter()
        .map(|label| label.as_str().to_owned())
        .collect()
}

fn parse_labels(values: Vec<String>) -> Result<BTreeSet<DataLabelId>, RepositoryError> {
    values
        .into_iter()
        .map(|value| {
            DataLabelId::new(value).map_err(|error| RepositoryError::Corrupt(error.to_string()))
        })
        .collect()
}

fn principal_kind(kind: PrincipalKind) -> platform::PrincipalKind {
    match kind {
        PrincipalKind::User => platform::PrincipalKind::User,
        PrincipalKind::Service => platform::PrincipalKind::Service,
    }
}

fn contract_principal_kind(kind: platform::PrincipalKind) -> PrincipalKind {
    match kind {
        platform::PrincipalKind::User => PrincipalKind::User,
        platform::PrincipalKind::Service => PrincipalKind::Service,
    }
}

fn grant_permission(level: AccessLevel) -> platform::GrantPermission {
    match level {
        AccessLevel::Read => platform::GrantPermission::Read,
        AccessLevel::Write => platform::GrantPermission::Write,
        AccessLevel::Admin => platform::GrantPermission::Admin,
    }
}

fn access_level(permission: platform::GrantPermission) -> AccessLevel {
    match permission {
        platform::GrantPermission::Read => AccessLevel::Read,
        platform::GrantPermission::Write => AccessLevel::Write,
        platform::GrantPermission::Admin => AccessLevel::Admin,
    }
}

fn platform_release_state(state: ArtifactReleaseState) -> platform::ArtifactReleaseState {
    match state {
        ArtifactReleaseState::Private => platform::ArtifactReleaseState::Private,
        ArtifactReleaseState::Releasable => platform::ArtifactReleaseState::Releasable,
        ArtifactReleaseState::Released => platform::ArtifactReleaseState::Released,
    }
}

fn release_state(state: platform::ArtifactReleaseState) -> ArtifactReleaseState {
    match state {
        platform::ArtifactReleaseState::Private => ArtifactReleaseState::Private,
        platform::ArtifactReleaseState::Releasable => ArtifactReleaseState::Releasable,
        platform::ArtifactReleaseState::Released => ArtifactReleaseState::Released,
    }
}

fn repository_error(error: platform::StoreError) -> RepositoryError {
    match error {
        platform::StoreError::ArtifactWriteConflict { .. }
        | platform::StoreError::ArtifactAccessRequestConflict(_) => {
            RepositoryError::Conflict(error.to_string())
        }
        other => RepositoryError::Backend(other.to_string()),
    }
}

fn record_uuid(record: &platform::RecordId) -> Result<uuid::Uuid, RepositoryError> {
    match &record.key {
        RecordIdKey::Uuid(value) => Ok(**value),
        _ => Err(RepositoryError::Corrupt(
            "artifact write reservation has a non-UUID identity".into(),
        )),
    }
}
