use crate::{AcceptedAuthority, Computer, ComputerError, Result, api::TerminalToken};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::time::Instant;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;

pub struct SessionGrantTicket {
    pub computer_id: Uuid,
    pub token: TerminalToken,
    pub expires_at: DateTime<Utc>,
}

/// In-process attachment authority. It cannot be deserialized from public input.
pub struct SessionGrantHandle {
    pub(super) grant_id: Uuid,
    pub(super) connection_id: Uuid,
}
impl SessionGrantHandle {
    pub fn grant_id(&self) -> Uuid {
        self.grant_id
    }
}

/// An observed access window, not permission to mutate lifecycle state.
pub struct SessionGrantLease {
    pub(super) computer: Computer,
    pub(super) checked_at: Instant,
    pub(super) valid_until: Instant,
}
impl SessionGrantLease {
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

#[derive(Deserialize, SurrealValue)]
pub(super) struct Record {
    pub id: RecordId,
    pub grant_id: Uuid,
    pub computer_id: Uuid,
    pub owner_key: String,
    pub provider_instance_id: Uuid,
    pub authority: OpenObject,
    pub family: RecordId,
    pub ticket_expires_at: DateTime<Utc>,
    pub connection_id: Option<Uuid>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub idle_expires_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub provider_resource_id: String,
    pub process_id: String,
}
impl Record {
    pub fn accepted(&self) -> Result<AcceptedAuthority> {
        let accepted: AcceptedAuthority = serde_json::from_value(
            serde_json::to_value(&self.authority).map_err(|_| ComputerError::Unavailable)?,
        )
        .map_err(|_| ComputerError::Unavailable)?;
        accepted.validate()?;
        let family_id = accepted
            .request_context
            .access_token
            .session_family
            .as_ref()
            .ok_or(ComputerError::Forbidden)?
            .as_str()
            .parse()
            .map_err(|_| ComputerError::Forbidden)?;
        if self.id != super::record(self.grant_id)
            || self.grant_id.is_nil()
            || crate::identity::owner_key(&accepted.task_owner())? != self.owner_key
            || self.family != veoveo_platform_store::gateway_refresh_family_record_id(family_id)
        {
            return Err(ComputerError::Forbidden);
        }
        Ok(accepted)
    }
}
