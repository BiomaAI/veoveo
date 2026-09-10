//! Named principal authority. Grant identifiers are references, never credentials.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AutomationPermission {
    Read,
    Execute,
    Start,
    Stop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationExecutionLimits {
    #[schemars(range(min = 1, max = 7200))]
    pub maximum_seconds: u32,
    #[schemars(range(min = 1, max = 67108864))]
    pub maximum_output_bytes: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IssueAutomationGrantInput {
    pub computer_id: Uuid,
    pub request_id: Uuid,
    #[schemars(length(min = 1, max = 2048))]
    pub principal_id: String,
    #[schemars(length(min = 1, max = 256))]
    pub oauth_client_id: String,
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
    #[schemars(length(min = 1, max = 4))]
    pub permissions: BTreeSet<AutomationPermission>,
    pub execution_limits: Option<AutomationExecutionLimits>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationGrantView {
    pub computer_id: Uuid,
    pub grant_id: Uuid,
    pub principal_id: String,
    pub oauth_client_id: String,
    pub name: String,
    pub permissions: BTreeSet<AutomationPermission>,
    pub execution_limits: Option<AutomationExecutionLimits>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationGrantCollection {
    pub computer_id: Uuid,
    #[schemars(length(max = 64))]
    pub grants: Vec<AutomationGrantView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeAutomationGrantInput {
    pub computer_id: Uuid,
    pub grant_id: Uuid,
}
