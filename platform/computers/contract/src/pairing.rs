//! Explicit browser confirmation for the qualified stock CLI loopback adapter.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CliPairingInput {
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
    #[schemars(regex(pattern = "^[A-HJ-NP-Z2-9]{3}-[A-HJ-NP-Z2-9]{4}$"))]
    pub code: String,
    #[schemars(range(min = 1024, max = 65535))]
    pub callback_port: u16,
}
impl CliPairingInput {
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CliPairingChallenge {
    pub computer_id: Uuid,
    pub pairing_id: Uuid,
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
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct CliPairingToken(#[schemars(length(min = 107, max = 107))] String);
impl CliPairingToken {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CliPairingResult {
    pub computer_id: Uuid,
    pub pairing_id: Uuid,
    pub grant_id: Uuid,
    pub token: CliPairingToken,
    #[schemars(range(min = 1024, max = 65535))]
    pub callback_port: u16,
    pub expires_at: DateTime<Utc>,
}
