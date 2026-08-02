use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{EpochId, FrameRevision, PoseLimits, SessionId, Sha256Digest};

pub const POSE_INGRESS_CONTROL_SCHEMA: &str = "veoveo.io/simulation-view-pose-ingress-control/v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoseProducerAuthorization {
    pub producer_id: String,
    pub spiffe_id: String,
    pub authorization_revision: u64,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoseIngressLimits {
    pub maximum_entities: u32,
    pub maximum_message_bytes: u32,
    pub maximum_cadence_hz: u32,
    pub stale_after_ms: u32,
}

impl TryFrom<PoseIngressLimits> for PoseLimits {
    type Error = crate::PoseError;

    fn try_from(value: PoseIngressLimits) -> Result<Self, Self::Error> {
        let limits = Self {
            max_entities: value.maximum_entities as usize,
            max_message_bytes: value.maximum_message_bytes as usize,
            max_cadence_hz: value.maximum_cadence_hz,
            stale_after: std::time::Duration::from_millis(u64::from(value.stale_after_ms)),
        };
        if limits.max_entities == 0
            || limits.max_message_bytes == 0
            || limits.max_cadence_hz == 0
            || limits.stale_after.is_zero()
        {
            return Err(crate::PoseError::SharedSlot(
                "pose ingress limits must be positive".to_owned(),
            ));
        }
        Ok(limits)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoseIngressBinding {
    pub schema_version: String,
    pub session_id: SessionId,
    pub epoch_id: EpochId,
    pub frame_revision: FrameRevision,
    pub entity_table_revision: u64,
    pub entity_table_digest: Sha256Digest,
    pub limits: PoseIngressLimits,
    pub producer: PoseProducerAuthorization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoseIngressStatus {
    pub schema_version: String,
    pub session_id: SessionId,
    pub epoch_id: EpochId,
    pub producer_id: String,
    pub producer_spiffe_id: String,
    pub authorization_revision: u64,
    pub authorized_until: DateTime<Utc>,
    pub revoked: bool,
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_snapshot_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoseIngressReadiness {
    pub ready: bool,
    pub protocol_schema: String,
    pub mutually_authenticated: bool,
}
