//! SQL-admitted metadata reads and service-owned access snapshots.
use super::*;
use veoveo_artifact_contract::ArtifactMetadataSnapshot;

impl<R: ArtifactRepository, S: BlobStore> ArtifactService<R, S> {
    pub(super) fn read_authority(
        caller: &PlaneCaller,
    ) -> Result<ArtifactReadAuthority, ArtifactPlaneError> {
        Ok(ArtifactReadAuthority {
            actor: Self::actor(caller)?,
            groups: caller
                .memberships
                .iter()
                .map(|membership| membership.group.clone())
                .collect(),
            clearance: caller.clearance().clone(),
            work_context: caller.identity.authority.work_context.clone(),
        })
    }

    pub(super) async fn read_metadata_snapshot(
        &self,
        caller: &PlaneCaller,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactMetadataSnapshot, ArtifactPlaneError> {
        let authority = Self::read_authority(caller)?;
        let actor = authority.actor.clone();
        let stored = self
            .repository
            .read_artifact(authority, artifact_id)
            .await
            .map_err(transport)?;
        let Some(stored) = stored else {
            self.audit(
                Some(&actor.audit),
                ArtifactAction::artifact(ArtifactActivity::Inspect, artifact_id)
                    .requested(AccessLevel::Read),
                AuditOutcome::Denied,
                AuditReason::NotFound,
            )
            .await?;
            return Err(ArtifactPlaneError::NotFound);
        };
        self.authorize(
            caller,
            &stored,
            ArtifactActivity::Inspect,
            AccessLevel::Read,
        )
        .await?;
        ArtifactMetadataSnapshot::new(stored.metadata, stored.grants, stored.metadata_updated_at)
            .map_err(transport)
    }
}
