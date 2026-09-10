//! Task-bound output authority with mandatory inherited sensitivity labels.
use super::*;

impl<R: ArtifactRepository, S: BlobStore> ArtifactService<R, S> {
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
        if request.required_data_labels.len() > 256 {
            return Err(ArtifactPlaneError::InvalidRequest(
                "output capability admits at most 256 required labels".into(),
            ));
        }
        let mut authority = caller.identity.authority.clone();
        authority
            .output_policy
            .data_labels
            .extend(request.required_data_labels);
        // Classification is also a mandatory MAC label. A later put may select a
        // presentation classification, but cannot remove the admitted policy label.
        if let Some(classification) = &authority.output_policy.classification {
            authority
                .output_policy
                .data_labels
                .insert(classification.clone());
        }
        if !authority
            .output_policy
            .data_labels
            .is_subset(caller.clearance())
        {
            return Err(ArtifactPlaneError::InvalidRequest(
                "output capability labels exceed caller clearance".into(),
            ));
        }
        let capability_id = ArtifactWriteCapabilityId::new();
        let secret = random_secret()?;
        self.repository
            .create_write_capability(WriteCapabilityDraft {
                capability_id,
                actor: actor.clone(),
                authority,
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
}
