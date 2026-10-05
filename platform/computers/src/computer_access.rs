//! Bounded current Computer reads for retained owners and named grantees.
mod admission;
use crate::{
    Computer, ComputerActor, ComputerError, ComputersStore, ControlAuthority, Result,
    api::{AutomationPermission, ComputerAccessMode, ComputerGrantedAccess},
};
use std::time::{Duration, Instant};
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_platform_store::deterministic_principal_id;

pub struct ComputerReadPage {
    pub computers: Vec<ComputerReadAccess>,
    pub next_cursor: Option<veoveo_computers_contract::ComputerId>,
}

pub struct ComputerReadAccess {
    computer: Computer,
    mode: ComputerAccessMode,
    grants: Vec<ComputerGrantedAccess>,
    file_transfer: bool,
    deadline: Instant,
    read_permits: Vec<crate::automation_grants::GrantReadPermit>,
}
impl ComputerReadAccess {
    pub fn computer(&self) -> Result<&Computer> {
        self.check()?;
        Ok(&self.computer)
    }
    pub fn mode(&self) -> ComputerAccessMode {
        self.mode
    }
    pub fn grants(&self) -> Result<&[ComputerGrantedAccess]> {
        self.check()?;
        Ok(&self.grants)
    }
    pub fn allows_file_transfer(&self) -> Result<bool> {
        self.check()?;
        Ok(self.file_transfer)
    }
    pub fn valid_until(&self) -> Instant {
        self.deadline
    }
    fn check(&self) -> Result<()> {
        if self.deadline <= Instant::now() {
            return Err(ComputerError::Forbidden);
        }
        Ok(())
    }
}

pub(crate) fn scope(accepted: &crate::AcceptedAuthority) -> Result<Vec<(&'static str, Value)>> {
    accepted.validate()?;
    let source = &accepted.request_context.principal;
    Ok(vec![
        (
            "grantee_tenant",
            accepted.invocation.tenant.to_string().into_value(),
        ),
        ("grantee_principal", source.id.to_string().into_value()),
        (
            "grantee_kind",
            match source.kind {
                veoveo_mcp_contract::PrincipalKind::User => {
                    veoveo_platform_store::PrincipalKind::User
                }
                veoveo_mcp_contract::PrincipalKind::Service => {
                    veoveo_platform_store::PrincipalKind::Service
                }
            }
            .into_value(),
        ),
        ("grantee_issuer", source.issuer.to_string().into_value()),
        ("grantee_subject", source.subject.to_string().into_value()),
        (
            "grantee_labels",
            source
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .into_value(),
        ),
        (
            "grantee",
            deterministic_principal_id(accepted.invocation.tenant.as_str(), source.id.as_str())
                .map_err(|_| ComputerError::Forbidden)?
                .record_id()
                .into_value(),
        ),
        (
            "client",
            accepted
                .request_context
                .access_token
                .oauth_client_id
                .as_str()
                .to_owned()
                .into_value(),
        ),
        (
            "context",
            accepted
                .invocation
                .work_context
                .as_str()
                .to_owned()
                .into_value(),
        ),
        ("profile", accepted.profile.as_str().to_owned().into_value()),
    ])
}

impl ComputersStore {
    /// A grant-change wake may invalidate the recipient's collection after
    /// revocation. It discloses no Computer state and authorizes no exact read.
    pub async fn automation_change_recipient(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        computer: veoveo_computers_contract::ComputerId,
        grant: crate::api::AutomationGrantId,
    ) -> Result<bool> {
        control.require_actor(actor)?;
        control.require_read(None)?;
        let mut params = scope(actor.accepted())?;
        params.extend([
            ("computer", computer.as_uuid().into_value()),
            (
                "grant",
                crate::automation_grants::record(grant).into_value(),
            ),
            ("provider", self.provider_instance_id.as_uuid().into_value()),
        ]);
        let mut read = self
            .query(
                include_str!("../queries/computer_access/automation_change_recipient.surql"),
                params,
            )
            .await?;
        let ids: Vec<Uuid> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        control.require_read(None)?;
        Ok(ids == [grant.as_uuid()])
    }
    pub async fn read_accessible_computers(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        after: Option<crate::api::ComputerId>,
        limit: u32,
    ) -> Result<ComputerReadPage> {
        self.read_accessible_page(actor, control, after, limit, "")
            .await
    }

    pub async fn complete_accessible_ids(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        prefix: &str,
    ) -> Result<(Vec<String>, bool)> {
        if prefix.len() > 36
            || !prefix
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c) || c == b'-')
        {
            return Err(ComputerError::InvalidInput);
        }
        let page = self
            .read_accessible_page(actor, control, None, 100, prefix)
            .await?;
        let ids = page
            .computers
            .iter()
            .map(|c| Ok(c.computer()?.computer_id.to_string()))
            .collect::<Result<Vec<_>>>()?;
        Ok((ids, page.next_cursor.is_some()))
    }

    async fn read_accessible_page(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        after: Option<crate::api::ComputerId>,
        limit: u32,
        prefix: &str,
    ) -> Result<ComputerReadPage> {
        let deadline = control
            .valid_until()
            .min(Instant::now() + Duration::from_secs(5));
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
            if !(1..=100).contains(&limit) {
                return Err(ComputerError::InvalidInput);
            }
            let mut scan_after = after;
            let mut admitted = Vec::new();
            loop {
                // This is a private authorization-key batch, not a public page.
                // Only SQL-admitted Computer rows can consume the caller's limit.
                let (ids, more) = self
                    .accessible_computer_candidates(actor, control, scan_after, 100, prefix)
                    .await?;
                let mut exhausted = more.is_none();
                for (position, id) in ids.iter().copied().enumerate() {
                    scan_after = Some(id);
                    match self.resolve_computer_read(actor, control, id).await {
                        Ok(access) => admitted.push(access),
                        Err(ComputerError::Forbidden | ComputerError::NotFound) => {}
                        Err(error) => return Err(error),
                    }
                    if admitted.len() > limit as usize {
                        exhausted = exhausted && position + 1 == ids.len();
                        break;
                    }
                }
                admitted = self
                    .select_admitted_computers(actor, control, admitted, limit + 1)
                    .await?;
                if admitted.len() > limit as usize || exhausted {
                    let more = admitted.len() > limit as usize;
                    admitted.truncate(limit as usize);
                    let next_cursor =
                        more.then(|| admitted.last().expect("nonzero page").computer.computer_id);
                    for access in &admitted {
                        access.check()?;
                    }
                    control.require_read(None)?;
                    return Ok(ComputerReadPage {
                        computers: admitted,
                        next_cursor,
                    });
                }
            }
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    /// One request-scoped source decision is shared by its collection rows. Every
    /// named grant still checks its owner, current policy and retained labels.
    pub async fn read_computer_access(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        id: crate::api::ComputerId,
    ) -> Result<ComputerReadAccess> {
        let deadline = control
            .valid_until()
            .min(Instant::now() + Duration::from_secs(5));
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
            let access = self.resolve_computer_read(actor, control, id).await?;
            self.select_admitted_computers(actor, control, vec![access], 1)
                .await?
                .pop()
                .ok_or(ComputerError::NotFound)
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    async fn resolve_computer_read(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        id: crate::api::ComputerId,
    ) -> Result<ComputerReadAccess> {
        control.require_actor(actor)?;
        control.require_read(Some(id))?;
        let deadline = control
            .valid_until()
            .min(Instant::now() + Duration::from_secs(5));
        match self.get(actor.owner(), id).await {
            Ok(computer) => {
                return Ok(ComputerReadAccess {
                    computer,
                    mode: ComputerAccessMode::Owner,
                    grants: vec![],
                    file_transfer: control.allows_file_transfer(),
                    deadline,
                    read_permits: vec![],
                });
            }
            Err(ComputerError::NotFound) => {}
            Err(error) => return Err(error),
        }
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
            // Resolve a usable Read grant before decoding any Computer row. Other
            // grants can contribute action choices only after this read is admitted.
            let read_ids = self.grantee_grant_ids(actor, id, true).await?;
            let mut computer = None;
            let mut grants = Vec::new();
            let mut expiry = deadline;
            let mut file_transfer = false;
            let mut read_permits = Vec::new();
            let mut authorities = Vec::new();
            for grant in read_ids {
                match self
                    .authorize_automation_grant(actor, id, grant, AutomationPermission::Read)
                    .await
                {
                    Ok(authority) => {
                        control.require_same_revision(authority.control_revision())?;
                        read_permits.push(authority.read_permit()?);
                        authorities.push(authority);
                    }
                    Err(ComputerError::Forbidden | ComputerError::NotFound) => {}
                    Err(error) => return Err(error),
                }
            }
            if authorities.is_empty() {
                return Err(ComputerError::NotFound);
            }
            for grant in self.grantee_grant_ids(actor, id, false).await? {
                if authorities
                    .iter()
                    .any(|authority| authority.grant_id() == grant)
                {
                    continue;
                }
                match self.automation_access_scope(actor, id, grant).await {
                    Ok(authority) => {
                        control.require_same_revision(authority.control_revision())?;
                        authorities.push(authority);
                    }
                    Err(ComputerError::Forbidden | ComputerError::NotFound) => {}
                    Err(error) => return Err(error),
                }
            }
            for authority in authorities {
                let selected = authority.computer()?;
                if let Some(prior) = &computer {
                    if prior != selected {
                        return Err(ComputerError::StateConflict);
                    }
                } else {
                    computer = Some(selected.clone());
                }
                let can_transfer_files = authority.allows_file_transfer()?;
                file_transfer |= can_transfer_files;
                expiry = expiry.min(authority.valid_until());
                let current = authority.current_view()?;
                grants.push(ComputerGrantedAccess {
                    grant_id: current.grant_id,
                    name: current.name.clone(),
                    permissions: current.permissions.clone(),
                    execution_limits: current.execution_limits,
                    can_transfer_files,
                    expires_at: current.expires_at,
                });
            }
            grants.sort_by_key(|grant| grant.grant_id);
            if !grants
                .iter()
                .any(|grant| grant.permissions.contains(&AutomationPermission::Read))
            {
                return Err(ComputerError::NotFound);
            }
            control.require_read(Some(id))?;
            let access = ComputerReadAccess {
                computer: computer.ok_or(ComputerError::NotFound)?,
                mode: ComputerAccessMode::Granted,
                grants,
                file_transfer,
                deadline: expiry,
                read_permits,
            };
            access.check()?;
            Ok(access)
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    async fn grantee_grant_ids(
        &self,
        actor: &ComputerActor,
        computer: crate::api::ComputerId,
        read_only: bool,
    ) -> Result<Vec<crate::api::AutomationGrantId>> {
        let mut params = scope(actor.accepted())?;
        params.extend([
            ("computer", computer.as_uuid().into_value()),
            (
                "computer_record",
                crate::model::computer_record(computer).into_value(),
            ),
            ("provider", self.provider_instance_id.as_uuid().into_value()),
            ("read_only", read_only.into_value()),
        ]);
        let mut read = self
            .query(
                include_str!("../queries/computer_access_grants.surql"),
                params,
            )
            .await?;
        let index = read
            .num_statements()
            .checked_sub(1)
            .ok_or(ComputerError::Unavailable)?;
        let ids: Vec<Uuid> = read.take(index).map_err(|_| ComputerError::Unavailable)?;
        if ids.len() > 64 {
            return Err(ComputerError::Unavailable);
        }
        ids.into_iter()
            .map(|id| {
                crate::api::AutomationGrantId::try_from(id).map_err(|_| ComputerError::Unavailable)
            })
            .collect()
    }

    /// Scan private authorization keys. Public page limits belong to the final
    /// SQL read after current policy resolves the admission permits.
    async fn accessible_computer_candidates(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        after: Option<crate::api::ComputerId>,
        limit: u32,
        prefix: &str,
    ) -> Result<(Vec<crate::api::ComputerId>, Option<crate::api::ComputerId>)> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        control.require_actor(actor)?;
        control.require_read(None)?;
        let mut params = scope(actor.accepted())?;
        params.extend(crate::store::owner_query_bindings(actor.owner())?);
        params.extend([
            ("provider", self.provider_instance_id.as_uuid().into_value()),
            (
                "after",
                after.map(crate::api::ComputerId::as_uuid).into_value(),
            ),
            ("limit", i64::from(limit + 1).into_value()),
            ("prefix", prefix.to_owned().into_value()),
        ]);
        let mut read = self
            .query(
                include_str!("../queries/accessible_computers.surql"),
                params,
            )
            .await?;
        let result = read
            .num_statements()
            .checked_sub(1)
            .ok_or(ComputerError::Unavailable)?;
        let ids: Vec<Uuid> = read.take(result).map_err(|_| ComputerError::Unavailable)?;
        let mut ids = ids
            .into_iter()
            .map(crate::api::ComputerId::try_from)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| ComputerError::Unavailable)?;
        let more = ids.len() > limit as usize;
        ids.truncate(limit as usize);
        let next = more.then(|| *ids.last().expect("nonzero limit"));
        control.require_read(None)?;
        Ok((ids, next))
    }
}
