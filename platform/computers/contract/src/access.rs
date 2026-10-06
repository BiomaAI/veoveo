//! Safe access inventory. Grant identifiers locate records and confer no authority.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccessGrantKind {
    Browser,
    Cli,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "AccessGrantView")]
pub struct AccessGrantViewValue {
    pub grant_id: crate::AccessGrantId,
    pub kind: AccessGrantKind,
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
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
#[schemars(rename = "AccessGrantCollection")]
pub struct AccessGrantCollectionValue {
    pub computer_id: crate::ComputerId,
    #[schemars(length(max = 128))]
    pub grants: Vec<AccessGrantView>,
}

/// Revocation accepts an interactive access identity.
///
/// ```compile_fail
/// use veoveo_computers_contract::{AutomationGrantId, ComputerId, RevokeAccessInput};
/// let input = RevokeAccessInput {
///     computer_id: ComputerId::new(),
///     grant_id: AutomationGrantId::new(),
/// };
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeAccessInput {
    pub computer_id: crate::ComputerId,
    pub grant_id: crate::AccessGrantId,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeAccessBody {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessRevocation {
    pub computer_id: crate::ComputerId,
    pub grant_id: crate::AccessGrantId,
    pub revoked: bool,
}

/// Admitted AccessGrantView; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AccessGrantViewValue", into = "AccessGrantViewValue")]
pub struct AccessGrantView(veoveo_types::Checked<AccessGrantViewValue>);
impl std::ops::Deref for AccessGrantView {
    type Target = AccessGrantViewValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for AccessGrantView {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AccessGrantView".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        AccessGrantViewValue::json_schema(generator)
    }
}
impl TryFrom<AccessGrantViewValue> for AccessGrantView {
    type Error = crate::ComputerResultError;
    fn try_from(value: AccessGrantViewValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<AccessGrantView> for AccessGrantViewValue {
    fn from(value: AccessGrantView) -> Self {
        value.0.into_inner()
    }
}
impl AccessGrantViewValue {
    pub fn build(self) -> Result<AccessGrantView, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for AccessGrantViewValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        crate::value_admission::name(&self.name)?;
        if self.expires_at <= self.issued_at || self.last_activity_at < self.issued_at {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}

/// Admitted AccessGrantCollection; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "AccessGrantCollectionValue",
    into = "AccessGrantCollectionValue"
)]
pub struct AccessGrantCollection(veoveo_types::Checked<AccessGrantCollectionValue>);
impl std::ops::Deref for AccessGrantCollection {
    type Target = AccessGrantCollectionValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for AccessGrantCollection {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AccessGrantCollection".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        AccessGrantCollectionValue::json_schema(generator)
    }
}
impl TryFrom<AccessGrantCollectionValue> for AccessGrantCollection {
    type Error = crate::ComputerResultError;
    fn try_from(value: AccessGrantCollectionValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<AccessGrantCollection> for AccessGrantCollectionValue {
    fn from(value: AccessGrantCollection) -> Self {
        value.0.into_inner()
    }
}
impl AccessGrantCollectionValue {
    pub fn build(self) -> Result<AccessGrantCollection, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for AccessGrantCollectionValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.grants.len() > 128
            || self
                .grants
                .iter()
                .map(|grant| grant.grant_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.grants.len()
        {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}
