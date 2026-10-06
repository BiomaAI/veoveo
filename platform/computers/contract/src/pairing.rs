//! Explicit browser confirmation for the qualified stock CLI loopback adapter.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "CliPairingInput")]
pub struct CliPairingInputValue {
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
    #[schemars(regex(pattern = "^[A-HJ-NP-Z2-9]{3}-[A-HJ-NP-Z2-9]{4}$"))]
    pub code: String,
    #[schemars(range(min = 1024, max = 65535))]
    pub callback_port: u16,
}
impl CliPairingInputValue {
    pub fn is_valid(&self) -> bool {
        let code = self.code.as_bytes();
        !self.name.is_empty()
            && self.name.len() <= 64
            && self.name.trim() == self.name
            && !self.name.chars().any(char::is_control)
            && self.callback_port >= 1024
            && code.len() == 8
            && code[3] == b'-'
            && code
                .iter()
                .enumerate()
                .all(|(i, c)| i == 3 || b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(c))
    }
}

/// The confirmation challenge keeps its identity separate from issued access.
///
/// ```compile_fail
/// use veoveo_computers_contract::{AccessGrantId, CliPairingId};
/// let pairing: CliPairingId = AccessGrantId::new();
/// ```
///
/// ```compile_fail
/// use veoveo_computers_contract::{AccessConnectionId, CliPairingId};
/// let pairing: CliPairingId = AccessConnectionId::new();
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CliPairingChallenge {
    pub computer_id: crate::ComputerId,
    pub pairing_id: crate::CliPairingId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CliPairingConfirmBody {}

/// Only serialize into the one-use no-store response and local callback body.
///
/// ```compile_fail
/// use veoveo_computers_contract::CliPairingToken;
/// fn cannot_log(token: CliPairingToken) { let _ = format!("{token:?}"); }
/// ```
pub struct CliPairingToken {
    wire: String,
    grant_id: crate::AccessGrantId,
}
impl Serialize for CliPairingToken {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.wire.serialize(serializer)
    }
}
impl JsonSchema for CliPairingToken {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CliPairingToken".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"string", "minLength":107, "maxLength":107})
    }
}
impl CliPairingToken {
    pub fn new(value: String) -> Result<Self, crate::ComputerResultError> {
        let mut parts = value.split('.');
        let prefix = parts.next();
        let id = parts.next().ok_or(crate::ComputerResultError)?;
        let secret = parts.next().ok_or(crate::ComputerResultError)?;
        let admitted: crate::AccessGrantId = id.parse().map_err(|_| crate::ComputerResultError)?;
        if value.len() != 107
            || prefix != Some("vcli1")
            || parts.next().is_some()
            || admitted.to_string() != id
            || secret.len() != 64
            || !secret
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(crate::ComputerResultError);
        }
        Ok(Self {
            wire: value,
            grant_id: admitted,
        })
    }
    pub fn expose_secret(&self) -> &str {
        &self.wire
    }
    pub fn grant_id(&self) -> crate::AccessGrantId {
        self.grant_id
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "CliPairingResult")]
pub struct CliPairingResultValue {
    pub computer_id: crate::ComputerId,
    pub pairing_id: crate::CliPairingId,
    pub grant_id: crate::AccessGrantId,
    pub token: CliPairingToken,
    #[schemars(range(min = 1024, max = 65535))]
    pub callback_port: u16,
    pub expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(transparent)]
pub struct CliPairingResult(veoveo_types::Checked<CliPairingResultValue>);
impl std::ops::Deref for CliPairingResult {
    type Target = CliPairingResultValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for CliPairingResult {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CliPairingResult".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        CliPairingResultValue::json_schema(generator)
    }
}
impl veoveo_types::Check for CliPairingResultValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.callback_port < 1024 || self.token.grant_id() != self.grant_id {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}
impl CliPairingResultValue {
    pub fn build(self) -> Result<CliPairingResult, crate::ComputerResultError> {
        veoveo_types::Checked::new(self).map(CliPairingResult)
    }
}
impl<'de> Deserialize<'de> for CliPairingResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CliPairingResultValue::deserialize(deserializer)?
            .build()
            .map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for CliPairingToken {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Admitted CliPairingInput; callers assemble its value and build before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CliPairingInputValue", into = "CliPairingInputValue")]
pub struct CliPairingInput(veoveo_types::Checked<CliPairingInputValue>);
impl std::ops::Deref for CliPairingInput {
    type Target = CliPairingInputValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for CliPairingInput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CliPairingInput".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        CliPairingInputValue::json_schema(generator)
    }
}
impl TryFrom<CliPairingInputValue> for CliPairingInput {
    type Error = crate::ComputerResultError;
    fn try_from(value: CliPairingInputValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<CliPairingInput> for CliPairingInputValue {
    fn from(value: CliPairingInput) -> Self {
        value.0.into_inner()
    }
}
impl CliPairingInputValue {
    pub fn build(self) -> Result<CliPairingInput, crate::ComputerResultError> {
        self.try_into()
    }
}
impl veoveo_types::Check for CliPairingInputValue {
    type Error = crate::ComputerResultError;
    fn check(&self) -> Result<(), Self::Error> {
        if !self.is_valid() {
            return Err(crate::ComputerResultError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod result_tests {
    use super::*;
    #[test]
    fn pairing_result_binds_admitted_token_grant_and_loopback_port() {
        let grant = crate::AccessGrantId::new();
        let draft = |grant_id, callback_port| CliPairingResultValue {
            computer_id: crate::ComputerId::new(),
            pairing_id: crate::CliPairingId::new(),
            grant_id,
            token: CliPairingToken::new(format!("vcli1.{grant}.{}", "a".repeat(64))).unwrap(),
            callback_port,
            expires_at: Utc::now(),
        };
        let valid = draft(grant, 1024).build().unwrap();
        let bytes = serde_json::to_vec(&valid).unwrap();
        assert!(serde_json::from_slice::<CliPairingResult>(&bytes).is_ok());
        for invalid in [draft(crate::AccessGrantId::new(), 1024), draft(grant, 1023)] {
            let bytes = serde_json::to_vec(&invalid).unwrap();
            assert!(invalid.build().is_err());
            assert!(serde_json::from_slice::<CliPairingResult>(&bytes).is_err());
        }
    }
}
