use super::*;
use crate::ledger::{
    ReadCapabilityAuthentication, ReadCapabilityDraft, ReadCapabilityRepository, ReadContextVersion,
};
use veoveo_mcp_contract::{ArtifactReadCapabilityId, ArtifactTaskId, GroupMembership, GroupRole};

impl ReadCapabilityRepository for SurrealArtifactRepository {
    async fn read_context(
        &self,
        actor: &RepositoryActor,
        context: &WorkContextId,
    ) -> Result<Option<ReadContextVersion>, RepositoryError> {
        self.store
            .artifact_read_context_version(actor.tenant.as_str(), context.as_str())
            .await
            .map_err(repository_error)?
            .map(|context| {
                Ok(ReadContextVersion {
                    policy_revision: PolicyVersion::new(context.policy_revision)
                        .map_err(corrupt)?,
                    digest: context.digest,
                })
            })
            .transpose()
    }
    async fn create_read_capability(
        &self,
        draft: ReadCapabilityDraft,
    ) -> Result<(), RepositoryError> {
        self.store
            .create_artifact_read_capability(platform::ArtifactReadCapabilityDraft {
                capability_id: platform::ArtifactReadCapabilityId::from_uuid(
                    draft.capability_id.as_uuid(),
                ),
                identity: self.identity(&draft.actor).await?,
                authority: platform_authority(&draft.authority),
                context_digest: draft.context.digest,
                actor_kind: principal_kind(draft.actor.kind),
                actor_issuer: draft.actor.issuer.to_string(),
                actor_subject: draft.actor.subject.to_string(),
                profile_key: draft.profile.to_string(),
                server_key: draft.server.to_string(),
                task_id: draft.task_id.as_uuid(),
                token_hash: draft.token_hash,
                labels: draft.labels.iter().map(ToString::to_string).collect(),
                memberships: draft
                    .memberships
                    .iter()
                    .map(|member| platform::ArtifactReadMembership {
                        group_key: member.group.to_string(),
                        permission: match member.role {
                            GroupRole::Read => platform::GrantPermission::Read,
                            GroupRole::Write => platform::GrantPermission::Write,
                            GroupRole::Admin => platform::GrantPermission::Admin,
                        },
                    })
                    .collect(),
                max_artifact_count: i64::from(draft.max_artifact_count),
                max_total_bytes: i64::try_from(draft.max_total_bytes).map_err(corrupt)?,
                expires_at: draft.expires_at,
            })
            .await
            .map_err(repository_error)
    }
    async fn read_capability(
        &self,
        authentication: &ReadCapabilityAuthentication,
    ) -> Result<Option<ReadCapabilityDraft>, RepositoryError> {
        self.store
            .artifact_read_capability(
                platform::ArtifactReadCapabilityId::from_uuid(
                    authentication.capability_id.as_uuid(),
                ),
                &authentication.token_hash,
                authentication.task_id.as_uuid(),
            )
            .await
            .map_err(repository_error)?
            .map(|record| decode(authentication.capability_id, record))
            .transpose()
    }
    async fn admit_read_capability(
        &self,
        authentication: &ReadCapabilityAuthentication,
        artifact: ArtifactId,
        byte_len: u64,
    ) -> Result<bool, RepositoryError> {
        self.store
            .admit_artifact_read(
                platform::ArtifactReadCapabilityId::from_uuid(
                    authentication.capability_id.as_uuid(),
                ),
                &authentication.token_hash,
                authentication.task_id.as_uuid(),
                platform::ArtifactId::from_uuid(artifact.as_uuid()),
                i64::try_from(byte_len).map_err(corrupt)?,
            )
            .await
            .map_err(repository_error)
    }
    async fn revoke_read_capability(
        &self,
        capability: ArtifactReadCapabilityId,
        actor: &RepositoryActor,
    ) -> Result<bool, RepositoryError> {
        self.store
            .revoke_artifact_read_capability(
                platform::ArtifactReadCapabilityId::from_uuid(capability.as_uuid()),
                &self.identity(actor).await?,
            )
            .await
            .map_err(repository_error)
    }
}

fn corrupt(error: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::Corrupt(error.to_string())
}

fn decode(
    id: ArtifactReadCapabilityId,
    record: platform::ArtifactReadCapabilityRecord,
) -> Result<ReadCapabilityDraft, RepositoryError> {
    let tenant = TenantId::new(record.tenant_key).map_err(corrupt)?;
    let authority = contract_authority(tenant.clone(), record.authority)?;
    Ok(ReadCapabilityDraft {
        capability_id: id,
        actor: RepositoryActor {
            tenant,
            principal: PrincipalId::new(record.actor_key).map_err(corrupt)?,
            kind: match record.actor_kind {
                platform::PrincipalKind::User => PrincipalKind::User,
                platform::PrincipalKind::Service => PrincipalKind::Service,
            },
            issuer: TokenIssuer::new(record.actor_issuer).map_err(corrupt)?,
            subject: TokenSubject::new(record.actor_subject).map_err(corrupt)?,
        },
        context: ReadContextVersion {
            policy_revision: authority.policy_revision.clone(),
            digest: record.context_digest,
        },
        authority,
        profile: GatewayProfileId::new(record.profile_key).map_err(corrupt)?,
        server: ServerSlug::new(record.server_key).map_err(corrupt)?,
        task_id: ArtifactTaskId::parse(record.task_id.to_string()).map_err(corrupt)?,
        token_hash: record.token_hash,
        labels: parse_labels(record.labels)?,
        memberships: record
            .memberships
            .into_iter()
            .map(|member| {
                Ok(GroupMembership {
                    group: GroupId::new(member.group_key).map_err(corrupt)?,
                    role: match member.permission {
                        platform::GrantPermission::Read => GroupRole::Read,
                        platform::GrantPermission::Write => GroupRole::Write,
                        platform::GrantPermission::Admin => GroupRole::Admin,
                    },
                })
            })
            .collect::<Result<_, RepositoryError>>()?,
        max_artifact_count: u32::try_from(record.max_artifact_count).map_err(corrupt)?,
        max_total_bytes: u64::try_from(record.max_total_bytes).map_err(corrupt)?,
        expires_at: record.expires_at,
    })
}
