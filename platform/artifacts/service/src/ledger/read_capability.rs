use super::{RepositoryActor, RepositoryError};
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
use veoveo_mcp_contract::{
    ArtifactId, ArtifactReadCapabilityId, ArtifactTaskId, DataLabelId, GatewayProfileId,
    GroupMembership, InvocationAuthority, PolicyVersion, ServerSlug, WorkContextId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadContextVersion {
    pub policy_revision: PolicyVersion,
    pub digest: String,
}

#[derive(Clone, Debug)]
pub struct ReadCapabilityDraft {
    pub capability_id: ArtifactReadCapabilityId,
    pub actor: RepositoryActor,
    pub authority: InvocationAuthority,
    pub context: ReadContextVersion,
    pub profile: GatewayProfileId,
    pub server: ServerSlug,
    pub task_id: ArtifactTaskId,
    pub token_hash: String,
    pub labels: BTreeSet<DataLabelId>,
    pub memberships: BTreeSet<GroupMembership>,
    pub max_artifact_count: u32,
    pub max_total_bytes: u64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ReadCapabilityAuthentication {
    pub capability_id: ArtifactReadCapabilityId,
    pub token_hash: String,
    pub task_id: ArtifactTaskId,
}

pub trait ReadCapabilityRepository: Send + Sync {
    fn read_context(
        &self,
        actor: &RepositoryActor,
        context: &WorkContextId,
    ) -> impl std::future::Future<Output = Result<Option<ReadContextVersion>, RepositoryError>> + Send;
    fn create_read_capability(
        &self,
        draft: ReadCapabilityDraft,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;
    fn read_capability(
        &self,
        authentication: &ReadCapabilityAuthentication,
    ) -> impl std::future::Future<Output = Result<Option<ReadCapabilityDraft>, RepositoryError>> + Send;
    fn admit_read_capability(
        &self,
        authentication: &ReadCapabilityAuthentication,
        artifact: ArtifactId,
        byte_len: u64,
    ) -> impl std::future::Future<Output = Result<bool, RepositoryError>> + Send;
    fn revoke_read_capability(
        &self,
        capability: ArtifactReadCapabilityId,
        actor: &RepositoryActor,
    ) -> impl std::future::Future<Output = Result<bool, RepositoryError>> + Send;
}
