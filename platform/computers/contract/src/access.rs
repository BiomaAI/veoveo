//! Safe access inventory. Grant identifiers locate records and confer no authority.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccessGrantKind {
    Browser,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessGrantView {
    pub grant_id: Uuid,
    pub kind: AccessGrantKind,
    /// Redemption is recorded history, not a claim that a transport is connected.
    pub redeemed: bool,
    /// Issued under this sign-in family; it may belong to another browser tab.
    pub current_session: bool,
    pub issued_at: DateTime<Utc>,
    /// An upper bound. Current policy, idle expiry or revocation can end access sooner.
    pub expires_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessGrantCollection {
    pub computer_id: Uuid,
    #[schemars(length(max = 128))]
    pub grants: Vec<AccessGrantView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeAccessInput {
    pub computer_id: Uuid,
    pub grant_id: Uuid,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeAccessBody {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessRevocation {
    pub computer_id: Uuid,
    pub grant_id: Uuid,
    pub revoked: bool,
}
