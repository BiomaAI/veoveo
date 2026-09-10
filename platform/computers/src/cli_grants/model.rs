use crate::{AcceptedAuthority, Computer, ComputerError, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::time::Instant;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{OpenObject, gateway_refresh_family_record_id};

pub struct CliPairing {
    pub pairing_id: Uuid,
    pub computer_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

/// No formatting or implicit serialization of the bearer credential.
///
/// ```compile_fail
/// use veoveo_computers::cli_grants::CliGrantCredential;
/// fn cannot_log(token: CliGrantCredential) { let _ = format!("{token:?}"); }
/// ```
pub struct CliGrantCredential(String);
impl CliGrantCredential {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}
pub struct PairedCliGrant {
    pub grant_id: Uuid,
    pub computer_id: Uuid,
    pub credential: CliGrantCredential,
    pub callback_port: u16,
    pub expires_at: DateTime<Utc>,
}
pub struct CliConnectionHandle {
    pub(super) grant_id: Uuid,
    pub(super) connection_id: Uuid,
}
impl CliConnectionHandle {
    pub fn grant_id(&self) -> Uuid {
        self.grant_id
    }
}
pub struct CliGrantLease {
    pub(super) session_family_id: Uuid,
    pub(super) computer: Computer,
    pub(super) checked_at: Instant,
    pub(super) valid_until: Instant,
}
impl CliGrantLease {
    pub fn session_family_id(&self) -> Uuid {
        self.session_family_id
    }
    pub fn computer(&self) -> &Computer {
        &self.computer
    }
    pub fn checked_at(&self) -> Instant {
        self.checked_at
    }
    pub fn valid_until(&self) -> Instant {
        self.valid_until
    }
}
pub struct CliGrantView {
    pub grant_id: Uuid,
    pub name: String,
    pub current_session: bool,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Deserialize, SurrealValue)]
pub(super) struct Grant {
    pub id: RecordId,
    pub grant_id: Uuid,
    pub computer_id: Uuid,
    pub owner_key: String,
    pub provider_instance_id: Uuid,
    pub authority: OpenObject,
    pub family: RecordId,
    pub name: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub idle_expires_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}
impl Grant {
    pub fn accepted(&self) -> Result<AcceptedAuthority> {
        let accepted: AcceptedAuthority = serde_json::from_value(
            serde_json::to_value(&self.authority).map_err(|_| ComputerError::Unavailable)?,
        )
        .map_err(|_| ComputerError::Unavailable)?;
        accepted.validate()?;
        let family = family(&accepted)?;
        if self.id != super::grant_record(self.grant_id)
            || self.grant_id.is_nil()
            || crate::identity::owner_key(&accepted.task_owner())? != self.owner_key
            || family != self.family
        {
            return Err(ComputerError::Forbidden);
        }
        Ok(accepted)
    }
}
#[derive(Deserialize, SurrealValue)]
pub(super) struct Pairing {
    pub pairing_id: Uuid,
    pub computer_id: Uuid,
    pub owner_key: String,
    pub family: RecordId,
    pub binding_hash: String,
    pub callback_port: u16,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}
#[derive(Deserialize, SurrealValue)]
pub(super) struct Connection {
    pub connection_id: Uuid,
    pub grant_id: Uuid,
    pub provider_resource_id: String,
    pub process_id: String,
    pub expires_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}
pub(super) fn family(accepted: &AcceptedAuthority) -> Result<RecordId> {
    let id = accepted
        .request_context
        .access_token
        .session_family
        .as_ref()
        .ok_or(ComputerError::Forbidden)?
        .as_str()
        .parse()
        .map_err(|_| ComputerError::Forbidden)?;
    Ok(gateway_refresh_family_record_id(id))
}
pub(super) fn binding_hash(accepted: &AcceptedAuthority) -> Result<String> {
    crate::identity::digest(&(
        &accepted.profile,
        &accepted.actor.id,
        &accepted.invocation.work_context,
        &accepted.request_context.access_token.oauth_client_id,
        &accepted.request_context.access_token.session_family,
    ))
}
