use crate::{
    AcceptedAuthority, ComputerError, Result,
    api::{
        AutomationExecutionLimits, AutomationGrantView, AutomationPermission,
        IssueAutomationGrantInput,
    },
    identity::owner_key,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::BTreeSet;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_mcp_contract::{OAuthClientId, PrincipalId};
use veoveo_platform_store::{OpenObject, PrincipalKind, deterministic_principal_id};

pub(super) fn scope(
    permissions: &BTreeSet<AutomationPermission>,
    limits: Option<AutomationExecutionLimits>,
) -> Result<()> {
    if permissions.is_empty()
        || permissions.len() > 4
        || permissions.contains(&AutomationPermission::Execute) != limits.is_some()
        || limits.is_some_and(|limits| {
            !(1..=7200).contains(&limits.maximum_seconds)
                || !(1..=67108864).contains(&limits.maximum_output_bytes)
        })
    {
        return Err(ComputerError::InvalidInput);
    }
    Ok(())
}
pub(super) fn validate(input: &IssueAutomationGrantInput) -> Result<PrincipalId> {
    if input.computer_id.is_nil()
        || input.request_id.is_nil()
        || input.name.trim() != input.name
        || input.name.is_empty()
        || input.name.len() > 64
        || input.name.chars().any(char::is_control)
        || input.principal_id.len() > 2048
        || input.oauth_client_id.len() > 256
    {
        return Err(ComputerError::InvalidInput);
    }
    scope(&input.permissions, input.execution_limits)?;
    OAuthClientId::new(input.oauth_client_id.clone()).map_err(|_| ComputerError::InvalidInput)?;
    PrincipalId::new(input.principal_id.clone()).map_err(|_| ComputerError::InvalidInput)
}
pub(super) fn permission_name(permission: AutomationPermission) -> &'static str {
    match permission {
        AutomationPermission::Read => "read",
        AutomationPermission::Execute => "execute",
        AutomationPermission::Start => "start",
        AutomationPermission::Stop => "stop",
    }
}
fn permission(value: &str) -> Result<AutomationPermission> {
    match value {
        "read" => Ok(AutomationPermission::Read),
        "execute" => Ok(AutomationPermission::Execute),
        "start" => Ok(AutomationPermission::Start),
        "stop" => Ok(AutomationPermission::Stop),
        _ => Err(ComputerError::Unavailable),
    }
}

#[derive(Deserialize, SurrealValue)]
pub(super) struct Record {
    id: RecordId,
    grant_id: Uuid,
    computer_id: Uuid,
    owner_key: String,
    provider_instance_id: Uuid,
    authority: OpenObject,
    grantee: RecordId,
    principal_id: String,
    oauth_client_id: String,
    grantee_issuer: String,
    grantee_subject: String,
    grantee_kind: PrincipalKind,
    name: String,
    permissions: Vec<String>,
    execution_limits: Option<OpenObject>,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    revision: u64,
}
pub(super) struct Grant {
    pub view: AutomationGrantView,
    pub owner_key: String,
    pub provider: Uuid,
    pub authority: AcceptedAuthority,
    pub grantee: RecordId,
    pub grantee_issuer: String,
    pub grantee_subject: String,
    pub grantee_kind: PrincipalKind,
    pub revision: u64,
}
impl TryFrom<Record> for Grant {
    type Error = ComputerError;
    fn try_from(row: Record) -> Result<Self> {
        let decode = || -> std::result::Result<_, serde_json::Error> {
            Ok((
                serde_json::from_value::<AcceptedAuthority>(serde_json::to_value(row.authority)?)?,
                row.execution_limits
                    .map(|value| {
                        serde_json::from_value::<AutomationExecutionLimits>(serde_json::to_value(
                            value,
                        )?)
                    })
                    .transpose()?,
            ))
        };
        let (authority, limits) = decode().map_err(|_| ComputerError::Unavailable)?;
        authority
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        let permissions = row
            .permissions
            .iter()
            .map(|s| permission(s))
            .collect::<Result<BTreeSet<_>>>()?;
        let input = IssueAutomationGrantInput {
            computer_id: row.computer_id,
            request_id: row.grant_id,
            principal_id: row.principal_id.clone(),
            oauth_client_id: row.oauth_client_id.clone(),
            name: row.name.clone(),
            permissions: permissions.clone(),
            execution_limits: limits,
            expires_at: row.expires_at,
        };
        validate(&input).map_err(|_| ComputerError::Unavailable)?;
        if row.id != super::record(row.grant_id)
            || row.grant_id.get_version_num() != 7
            || row.provider_instance_id.is_nil()
            || row.owner_key != owner_key(&authority.task_owner())?
            || permissions.len() != row.permissions.len()
            || row.expires_at <= row.issued_at
            || row.expires_at - row.issued_at > chrono::TimeDelta::days(1)
            || row.revoked_at.is_some_and(|time| time < row.issued_at)
            || row.grantee
                != deterministic_principal_id(
                    authority.invocation.tenant.as_str(),
                    &row.principal_id,
                )
                .map_err(|_| ComputerError::Unavailable)?
                .record_id()
            || authority.actor.id != authority.request_context.principal.id
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(Self {
            view: AutomationGrantView {
                computer_id: row.computer_id,
                grant_id: row.grant_id,
                principal_id: row.principal_id,
                oauth_client_id: row.oauth_client_id,
                name: row.name,
                permissions,
                execution_limits: limits,
                issued_at: row.issued_at,
                expires_at: row.expires_at,
                revoked_at: row.revoked_at,
            },
            owner_key: row.owner_key,
            provider: row.provider_instance_id,
            authority,
            grantee: row.grantee,
            grantee_issuer: row.grantee_issuer,
            grantee_subject: row.grantee_subject,
            grantee_kind: row.grantee_kind,
            revision: row.revision,
        })
    }
}
