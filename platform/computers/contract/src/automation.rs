//! Named principal authority. Grant identifiers are references, never credentials.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_types::{OAuthClientId, PrincipalId};

// The identity schema owns lexical admission; Computers bounds the field's size.
fn automation_oauth_client_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let mut schema = OAuthClientId::json_schema(generator);
    schema.insert("maxLength".into(), 256.into());
    schema
}

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
#[schemars(rename = "AutomationExecutionLimits")]
pub struct AutomationExecutionLimitsValue {
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
#[schemars(rename = "IssueAutomationGrantInput")]
/// Computer and request identities have separate admission and API roles.
/// ```compile_fail
/// use veoveo_computers_contract::{ComputerId, RequestId};
/// fn requires_computer(_: ComputerId) {}
/// requires_computer(RequestId::new());
/// ```
pub struct IssueAutomationGrantInputValue {
    pub computer_id: crate::ComputerId,
    pub request_id: crate::RequestId,
    #[schemars(length(min = 1, max = 2048))]
    pub principal_id: PrincipalId,
    #[schemars(schema_with = "automation_oauth_client_schema")]
    pub oauth_client_id: OAuthClientId,
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
#[schemars(rename = "AutomationGrantView")]
pub struct AutomationGrantViewValue {
    pub computer_id: crate::ComputerId,
    pub grant_id: crate::AutomationGrantId,
    pub principal_id: PrincipalId,
    pub oauth_client_id: OAuthClientId,
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
#[schemars(rename = "AutomationGrantCollection")]
pub struct AutomationGrantCollectionValue {
    pub computer_id: crate::ComputerId,
    pub can_grant: bool,
    pub can_revoke: bool,
    #[schemars(length(max = 4))]
    pub grantable_permissions: BTreeSet<AutomationPermission>,
    #[schemars(length(max = 128))]
    pub client_choices: Vec<AutomationClientChoice>,
    pub client_choices_truncated: bool,
    pub limits: AutomationGrantLimits,
    #[schemars(length(max = 64))]
    pub grants: Vec<AutomationGrantView>,
}

/// Registration metadata available to a Computer owner who may issue a grant.
/// A choice is a profile hint, never proof of the grantee's current authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationClientChoice {
    pub oauth_client_id: OAuthClientId,
    pub display_name: String,
    /// Automated clients authenticate as this canonical service principal.
    pub service_principal_id: Option<PrincipalId>,
}

/// Current usable action scope on a Computer already authorized for Read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "ComputerGrantedAccess")]
pub struct ComputerGrantedAccessValue {
    pub grant_id: crate::AutomationGrantId,
    pub name: String,
    #[serde(deserialize_with = "unique_permissions")]
    pub permissions: BTreeSet<AutomationPermission>,
    pub execution_limits: Option<AutomationExecutionLimits>,
    pub can_transfer_files: bool,
    pub expires_at: DateTime<Utc>,
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
    pub computer_id: crate::ComputerId,
    pub grant_id: crate::AutomationGrantId,
}

/// Addressable grant state, including an expired or revoked grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationGrantResult {
    #[serde(rename = "result_uri")]
    result_uri: crate::AutomationGrantUri,
    grant: AutomationGrantView,
}
impl AutomationGrantResult {
    pub fn result_uri(&self) -> crate::AutomationGrantUri {
        self.result_uri
    }
    pub fn grant(&self) -> &AutomationGrantView {
        &self.grant
    }
    pub fn into_grant(self) -> AutomationGrantView {
        self.grant
    }
}
impl From<AutomationGrantView> for AutomationGrantResult {
    fn from(grant: AutomationGrantView) -> Self {
        Self {
            result_uri: crate::AutomationGrantUri::new(grant.computer_id, grant.grant_id),
            grant,
        }
    }
}

impl<'de> Deserialize<'de> for AutomationGrantResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            #[serde(rename = "result_uri")]
            result_uri: crate::AutomationGrantUri,
            grant: AutomationGrantView,
        }
        let wire = Wire::deserialize(deserializer)?;
        let result = Self::from(wire.grant);
        if result.result_uri != wire.result_uri {
            return Err(serde::de::Error::custom(crate::ComputerResultError));
        }
        Ok(result)
    }
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

/// Admitted AutomationExecutionLimits; callers assemble its value and build before use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "AutomationExecutionLimitsValue",
    into = "AutomationExecutionLimitsValue"
)]
pub struct AutomationExecutionLimits(veoveo_types::Checked<AutomationExecutionLimitsValue>);
impl std::ops::Deref for AutomationExecutionLimits {
    type Target = AutomationExecutionLimitsValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for AutomationExecutionLimits {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AutomationExecutionLimits".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        AutomationExecutionLimitsValue::json_schema(generator)
    }
}
impl TryFrom<AutomationExecutionLimitsValue> for AutomationExecutionLimits {
    type Error = crate::ComputerResultError;
    fn try_from(value: AutomationExecutionLimitsValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<AutomationExecutionLimits> for AutomationExecutionLimitsValue {
    fn from(value: AutomationExecutionLimits) -> Self {
        value.0.into_inner()
    }
}
impl AutomationExecutionLimitsValue {
    pub fn build(self) -> Result<AutomationExecutionLimits, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for AutomationExecutionLimitsValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        if !(1..=7200).contains(&self.maximum_seconds)
            || !(1..=67108864).contains(&self.maximum_output_bytes)
        {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}

/// Admitted IssueAutomationGrantInput; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "IssueAutomationGrantInputValue",
    into = "IssueAutomationGrantInputValue"
)]
pub struct IssueAutomationGrantInput(veoveo_types::Checked<IssueAutomationGrantInputValue>);
impl std::ops::Deref for IssueAutomationGrantInput {
    type Target = IssueAutomationGrantInputValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for IssueAutomationGrantInput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "IssueAutomationGrantInput".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        IssueAutomationGrantInputValue::json_schema(generator)
    }
}
impl TryFrom<IssueAutomationGrantInputValue> for IssueAutomationGrantInput {
    type Error = crate::ComputerResultError;
    fn try_from(value: IssueAutomationGrantInputValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<IssueAutomationGrantInput> for IssueAutomationGrantInputValue {
    fn from(value: IssueAutomationGrantInput) -> Self {
        value.0.into_inner()
    }
}
impl IssueAutomationGrantInputValue {
    pub fn build(self) -> Result<IssueAutomationGrantInput, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for IssueAutomationGrantInputValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        crate::value_admission::grant_scope(&self.name, &self.permissions, self.execution_limits)?;
        if self.principal_id.as_str().len() > 2048 || self.oauth_client_id.as_str().len() > 256 {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}

/// Admitted AutomationGrantView; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "AutomationGrantViewValue",
    into = "AutomationGrantViewValue"
)]
pub struct AutomationGrantView(veoveo_types::Checked<AutomationGrantViewValue>);
impl std::ops::Deref for AutomationGrantView {
    type Target = AutomationGrantViewValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for AutomationGrantView {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AutomationGrantView".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        AutomationGrantViewValue::json_schema(generator)
    }
}
impl TryFrom<AutomationGrantViewValue> for AutomationGrantView {
    type Error = crate::ComputerResultError;
    fn try_from(value: AutomationGrantViewValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<AutomationGrantView> for AutomationGrantViewValue {
    fn from(value: AutomationGrantView) -> Self {
        value.0.into_inner()
    }
}
impl AutomationGrantViewValue {
    pub fn build(self) -> Result<AutomationGrantView, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for AutomationGrantViewValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        crate::value_admission::grant_scope(&self.name, &self.permissions, self.execution_limits)?;
        if self.principal_id.as_str().len() > 2048
            || self.oauth_client_id.as_str().len() > 256
            || self.expires_at <= self.issued_at
            || self.revoked_at.is_some_and(|at| at < self.issued_at)
        {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}

/// Admitted ComputerGrantedAccess; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "ComputerGrantedAccessValue",
    into = "ComputerGrantedAccessValue"
)]
pub struct ComputerGrantedAccess(veoveo_types::Checked<ComputerGrantedAccessValue>);
impl std::ops::Deref for ComputerGrantedAccess {
    type Target = ComputerGrantedAccessValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ComputerGrantedAccess {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ComputerGrantedAccess".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ComputerGrantedAccessValue::json_schema(generator)
    }
}
impl TryFrom<ComputerGrantedAccessValue> for ComputerGrantedAccess {
    type Error = crate::ComputerResultError;
    fn try_from(value: ComputerGrantedAccessValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ComputerGrantedAccess> for ComputerGrantedAccessValue {
    fn from(value: ComputerGrantedAccess) -> Self {
        value.0.into_inner()
    }
}
impl ComputerGrantedAccessValue {
    pub fn build(self) -> Result<ComputerGrantedAccess, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ComputerGrantedAccessValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        crate::value_admission::grant_scope(&self.name, &self.permissions, self.execution_limits)?;
        Ok(())
    }
}

/// Admitted AutomationGrantCollection; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "AutomationGrantCollectionValue",
    into = "AutomationGrantCollectionValue"
)]
pub struct AutomationGrantCollection(veoveo_types::Checked<AutomationGrantCollectionValue>);
impl std::ops::Deref for AutomationGrantCollection {
    type Target = AutomationGrantCollectionValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for AutomationGrantCollection {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AutomationGrantCollection".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        AutomationGrantCollectionValue::json_schema(generator)
    }
}
impl TryFrom<AutomationGrantCollectionValue> for AutomationGrantCollection {
    type Error = crate::ComputerResultError;
    fn try_from(value: AutomationGrantCollectionValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<AutomationGrantCollection> for AutomationGrantCollectionValue {
    fn from(value: AutomationGrantCollection) -> Self {
        value.0.into_inner()
    }
}
impl AutomationGrantCollectionValue {
    pub fn build(self) -> Result<AutomationGrantCollection, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for AutomationGrantCollectionValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.grants.len() > 64
            || self.client_choices.len() > 128
            || self
                .grants
                .iter()
                .any(|grant| grant.computer_id != self.computer_id)
            || self
                .grants
                .iter()
                .map(|grant| grant.grant_id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.grants.len()
            || self
                .client_choices
                .iter()
                .map(|client| &client.oauth_client_id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.client_choices.len()
        {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result() -> AutomationGrantResult {
        let now = "2026-10-04T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        crate::AutomationGrantViewValue {
            computer_id: crate::ComputerId::new(),
            grant_id: crate::AutomationGrantId::new(),
            principal_id: PrincipalId::parse("https://issuer.test#agent").unwrap(),
            oauth_client_id: OAuthClientId::parse("agent").unwrap(),
            name: "Reader".into(),
            permissions: [AutomationPermission::Read].into(),
            execution_limits: None,
            issued_at: now,
            expires_at: now + chrono::TimeDelta::minutes(10),
            revoked_at: None,
        }
        .build()
        .unwrap()
        .into()
    }

    #[test]
    fn grant_result_binds_both_parent_and_grant_and_preserves_wire_values() {
        let result = result();
        let uri = result.result_uri();
        assert_eq!(uri.computer_id(), result.grant().computer_id);
        assert_eq!(uri.grant_id(), result.grant().grant_id);
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(wire["result_uri"], String::from(uri));
        assert_eq!(wire["grant"]["principalId"], "https://issuer.test#agent");
        assert_eq!(
            serde_json::from_value::<AutomationGrantResult>(wire.clone()).unwrap(),
            result
        );
        for invalid_uri in [
            String::from(crate::AutomationGrantUri::new(
                crate::ComputerId::new(),
                uri.grant_id(),
            )),
            String::from(crate::AutomationGrantUri::new(
                uri.computer_id(),
                crate::AutomationGrantId::new(),
            )),
            crate::computer_uri(uri.computer_id()).to_string(),
            format!("{}?extra=true", uri.to_uri()),
        ] {
            let mut invalid = wire.clone();
            invalid["result_uri"] = invalid_uri.into();
            assert!(serde_json::from_value::<AutomationGrantResult>(invalid).is_err());
        }
        for field in ["principalId", "oauthClientId"] {
            for value in ["", "invalid\nidentity"] {
                let mut invalid = wire.clone();
                invalid["grant"][field] = value.into();
                assert!(serde_json::from_value::<AutomationGrantResult>(invalid).is_err());
            }
        }
    }

    #[test]
    fn grant_wire_requires_distinct_permissions_and_explicit_interruption_scope() {
        let grant = serde_json::json!({
            "computerId":crate::ComputerId::new(),"requestId":crate::RequestId::new(),"principalId":"agent","oauthClientId":"agent",
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
