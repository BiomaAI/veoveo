use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordingIngestResource {
    pub id: ProtectedResourceName,
    pub protected_resource: ProtectedResourceId,
    pub authorization_server: AuthorizationServerId,
    pub policy_version: PolicyVersion,
    pub upstream: UpstreamEndpoint,
    pub maximum_batch_bytes: u64,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_scopes: BTreeSet<ScopeName>,
    pub producers: Vec<RecordingProducerRegistration>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordingProducerRegistration {
    pub id: RecordingProducerId,
    pub oauth_client: OAuthClientId,
    pub tenant: TenantId,
    pub dataset: RecordingDatasetName,
    pub allowed_application_ids: BTreeSet<RecordingApplicationId>,
    pub classification: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub labels: BTreeSet<DataLabelId>,
    pub quotas: RecordingProducerQuotas,
    pub retention: RecordingRetentionPolicy,
    pub enabled: bool,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordingProducerQuotas {
    pub maximum_concurrent_streams: u32,
    pub maximum_batches_per_minute: u32,
    pub maximum_bytes_per_day: u64,
    pub maximum_stream_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordingRetentionPolicy {
    pub journal_grace_seconds: u32,
    pub open_stream_days: u32,
}
