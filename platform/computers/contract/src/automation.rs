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
#[serde(rename_all = "snake_case")]
pub enum AutomationInterruption {
    StopComputer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationExecutionLimits {
    #[schemars(range(min = 1, max = 7200))]
    pub maximum_seconds: u32,
    #[schemars(range(min = 1, max = 67108864))]
    pub maximum_output_bytes: u32,
    /// Cancellation, expiry or uncertain execution can stop this Computer run.
    /// Its retained files remain. This grants no independent agent Stop action.
    pub on_interruption: AutomationInterruption,
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
    #[serde(deserialize_with = "unique_permissions")]
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
    #[serde(deserialize_with = "unique_permissions")]
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
    pub can_grant: bool,
    pub can_revoke: bool,
    pub limits: AutomationGrantLimits,
    #[schemars(length(max = 64))]
    pub grants: Vec<AutomationGrantView>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationGrantLimits {
    pub maximum_grants: u32,
    pub maximum_lifetime_seconds: u32,
    pub maximum_execution_seconds: u32,
    pub maximum_output_bytes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeAutomationGrantBody {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeAutomationGrantInput {
    pub computer_id: Uuid,
    pub grant_id: Uuid,
}

/// Addressable grant state, including an expired or revoked grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationGrantResult {
    #[serde(rename = "result_uri")]
    pub result_uri: String,
    pub grant: AutomationGrantView,
}
impl From<AutomationGrantView> for AutomationGrantResult {
    fn from(grant: AutomationGrantView) -> Self {
        Self {
            result_uri: automation_grant_uri(grant.computer_id, grant.grant_id),
            grant,
        }
    }
}
pub fn automation_grant_uri(computer: Uuid, grant: Uuid) -> String {
    format!("computer://computers/{computer}/automation/{grant}")
}

fn unique_permissions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeSet<AutomationPermission>, D::Error> {
    struct Permissions;
    impl<'de> serde::de::Visitor<'de> for Permissions {
        type Value = BTreeSet<AutomationPermission>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("one to four distinct automation permissions")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = BTreeSet::new();
            while let Some(value) = sequence.next_element::<AutomationPermission>()? {
                if !values.insert(value) {
                    return Err(serde::de::Error::custom("duplicate automation permission"));
                }
            }
            if values.is_empty() {
                return Err(serde::de::Error::custom(
                    "automation permissions must not be empty",
                ));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Permissions)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grant_wire_requires_distinct_permissions_and_explicit_interruption_scope() {
        let grant = serde_json::json!({
            "computerId":Uuid::nil(),"requestId":Uuid::nil(),"principalId":"agent","oauthClientId":"agent",
            "name":"Builder","permissions":["read","execute"],"expiresAt":"2026-09-10T21:00:00Z",
            "executionLimits":{"maximumSeconds":30,"maximumOutputBytes":1024,"onInterruption":"stop_computer"}
        });
        let decoded: IssueAutomationGrantInput = serde_json::from_value(grant.clone()).unwrap();
        assert_eq!(decoded.permissions.len(), 2);
        for permissions in [
            serde_json::json!([]),
            serde_json::json!(["read", "read"]),
            serde_json::json!(["execute", "execute"]),
            serde_json::json!(["admin"]),
        ] {
            let mut invalid = grant.clone();
            invalid["permissions"] = permissions;
            assert!(serde_json::from_value::<IssueAutomationGrantInput>(invalid).is_err());
        }
        for scope in [None, Some("continue")] {
            let mut invalid = grant.clone();
            let limits = invalid["executionLimits"].as_object_mut().unwrap();
            limits.remove("onInterruption");
            if let Some(scope) = scope {
                limits.insert("onInterruption".into(), scope.into());
            }
            assert!(serde_json::from_value::<IssueAutomationGrantInput>(invalid).is_err());
        }
    }
}
