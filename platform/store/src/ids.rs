use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue, Uuid as SurrealUuid};
use uuid::Uuid;

macro_rules! domain_id {
    ($name:ident, $table:literal) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
            Deserialize,
            SurrealValue,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub const TABLE: &'static str = $table;

            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }

            pub fn record_id(self) -> RecordId {
                RecordId::new(Self::TABLE, SurrealUuid::from(self.0))
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl From<$name> for RecordId {
            fn from(value: $name) -> Self {
                value.record_id()
            }
        }
    };
}

domain_id!(EnterpriseId, "enterprise");
domain_id!(TenantId, "tenant");
domain_id!(PrincipalId, "principal");
domain_id!(GroupId, "principal_group");
domain_id!(OauthClientId, "oauth_client");
domain_id!(McpServerId, "mcp_server");
domain_id!(ProfileId, "profile");
domain_id!(PolicyRevisionId, "policy_revision");
domain_id!(WorkContextId, "work_context");
domain_id!(TaskId, "task");
domain_id!(ProviderJobId, "provider_job");
domain_id!(ProviderEventId, "provider_event");
domain_id!(ArtifactBlobId, "artifact_blob");
domain_id!(ArtifactId, "artifact_occurrence");
domain_id!(ShareLinkId, "share_link");
domain_id!(ArtifactWriteCapabilityId, "artifact_write_capability");
domain_id!(ArtifactWriteRedemptionId, "artifact_write_redemption");
domain_id!(ArtifactAccessRequestId, "artifact_access_request");
domain_id!(MediaTaskContextId, "media_task_context");
domain_id!(MediaUsageId, "media_usage");
domain_id!(DomainUsageId, "domain_usage");
domain_id!(FrameId, "frame");
domain_id!(CoordinateOperationId, "coordinate_operation");
domain_id!(RecordingId, "recording");
domain_id!(SegmentId, "segment");
domain_id!(RecordingIngestStreamId, "recording_ingest_stream");
domain_id!(RecordingIngestBatchId, "recording_ingest_batch");
domain_id!(AgentId, "agent");
domain_id!(WakeId, "wake");
domain_id!(AgentEpisodeId, "agent_episode");
domain_id!(AgentTaskId, "agent_task");
domain_id!(AgentElicitationId, "agent_elicitation");
domain_id!(AuditEventId, "audit_event");
domain_id!(OutboxEventId, "outbox_event");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_uuid_v7_and_table_scoped() {
        let id = ArtifactId::new();
        assert_eq!(id.as_uuid().get_version_num(), 7);
        let record = id.record_id();
        assert_eq!(record.table.as_str(), ArtifactId::TABLE);
    }

    #[test]
    fn ids_round_trip_through_json() {
        let id = TaskId::new();
        let encoded = serde_json::to_string(&id).unwrap();
        let decoded: TaskId = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, id);
    }
}
