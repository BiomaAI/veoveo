//! Bounded current Computer reads for retained owners and named grantees.
use crate::{
    Computer, ComputerActor, ComputerError, ComputersStore, ControlAuthority, Result,
    api::{AutomationPermission, ComputerAccessMode, ComputerGrantedAccess},
    identity::owner_key,
};
use std::time::{Duration, Instant};
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_platform_store::deterministic_principal_id;

pub struct ComputerReadPage {
    pub computers: Vec<ComputerReadAccess>,
    pub next_cursor: Option<Uuid>,
}

pub struct ComputerReadAccess {
    computer: Computer,
    mode: ComputerAccessMode,
    grants: Vec<ComputerGrantedAccess>,
    file_transfer: bool,
    deadline: Instant,
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

fn scope(actor: &ComputerActor) -> Result<Vec<(&'static str, Value)>> {
    let source = &actor.accepted().request_context.principal;
    Ok(vec![
        (
            "grantee",
            deterministic_principal_id(actor.owner().tenant_key(), source.id.as_str())
                .map_err(|_| ComputerError::Forbidden)?
                .record_id()
                .into_value(),
        ),
        (
            "client",
            actor
                .accepted()
                .request_context
                .access_token
                .oauth_client_id
                .as_str()
                .to_owned()
                .into_value(),
        ),
        (
            "context",
            actor
                .accepted()
                .invocation
                .work_context
                .as_str()
                .to_owned()
                .into_value(),
        ),
        (
            "profile",
            actor.accepted().profile.as_str().to_owned().into_value(),
        ),
    ])
}

impl ComputersStore {
    /// A grant-change wake may invalidate the recipient's collection after
    /// revocation. It discloses no Computer state and authorizes no exact read.
    pub async fn automation_change_recipient(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        computer: Uuid,
        grant: Uuid,
    ) -> Result<bool> {
        control.require_actor(actor)?;
        control.require_read(None)?;
        let mut params = scope(actor)?;
        params.extend([
            ("computer", computer.into_value()),
            (
                "grant",
                crate::automation_grants::record(grant).into_value(),
            ),
            ("provider", self.provider_instance_id.into_value()),
        ]);
        let mut read = self
            .query(
                "SELECT VALUE grant_id FROM $grant WHERE grantee = $grantee
             AND computer_id = $computer AND provider_instance_id = $provider
             AND oauth_client_id = $client AND authority.profile = $profile
             AND authority.invocation.work_context = $context AND 'read' IN permissions;",
                params,
            )
            .await?;
        let ids: Vec<Uuid> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        control.require_read(None)?;
        Ok(ids == [grant])
    }
    pub async fn read_accessible_computers(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        after: Option<Uuid>,
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
        after: Option<Uuid>,
        limit: u32,
        prefix: &str,
    ) -> Result<ComputerReadPage> {
        let deadline = control
            .valid_until()
            .min(Instant::now() + Duration::from_secs(5));
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
            let (ids, next_cursor) = self
                .accessible_computer_candidates(actor, control, after, limit, prefix)
                .await?;
            let records: Vec<_> = ids
                .iter()
                .copied()
                .map(crate::model::computer_record)
                .collect();
            let mut read = self
                .query(
                    "SELECT * FROM $computers;",
                    vec![("computers", records.into_value())],
                )
                .await?;
            let rows: Vec<crate::model::ComputerRecord> =
                read.take(0).map_err(|_| ComputerError::Unavailable)?;
            let key = owner_key(actor.owner())?;
            let mut computers = Vec::new();
            for row in rows {
                let computer = Computer::try_from(row)?;
                if !ids.contains(&computer.computer_id) {
                    return Err(ComputerError::Unavailable);
                }
                if owner_key(&computer.owner)? == key {
                    crate::identity::permits(&computer.owner, actor.owner())?;
                    control.require_read(Some(computer.computer_id))?;
                    computers.push(ComputerReadAccess {
                        computer,
                        mode: ComputerAccessMode::Owner,
                        grants: vec![],
                        file_transfer: control.allows_file_transfer(),
                        deadline,
                    });
                } else {
                    match self
                        .read_computer_access(actor, control, computer.computer_id)
                        .await
                    {
                        Ok(access) => computers.push(access),
                        Err(ComputerError::Forbidden | ComputerError::NotFound) => {}
                        Err(error) => return Err(error),
                    }
                }
            }
            computers.sort_by_key(|access| access.computer.computer_id);
            for access in &computers {
                access.check()?;
            }
            control.require_read(None)?;
            Ok(ComputerReadPage {
                computers,
                next_cursor,
            })
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
        id: Uuid,
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
                });
            }
            Err(ComputerError::NotFound) => {}
            Err(error) => return Err(error),
        }
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
            let mut params = scope(actor)?;
            params.extend([
                ("computer", id.into_value()),
                ("provider", self.provider_instance_id.into_value()),
            ]);
            let mut read = self
                .query(
                    include_str!("../queries/computer_access_grants.surql"),
                    params,
                )
                .await?;
            let ids: Vec<Uuid> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
            let mut computer = None;
            let mut grants = Vec::new();
            let mut expiry = deadline;
            let mut file_transfer = false;
            for grant in ids {
                let authority = match self.automation_access_scope(actor, id, grant).await {
                    Ok(authority) => authority,
                    Err(ComputerError::Forbidden | ComputerError::NotFound) => continue,
                    Err(error) => return Err(error),
                };
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
            };
            access.check()?;
            Ok(access)
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    /// Candidate cursor denotes the last scanned row. Current authorization is
    /// applied to every candidate before a facade returns any Computer metadata.
    async fn accessible_computer_candidates(
        &self,
        actor: &ComputerActor,
        control: &ControlAuthority,
        after: Option<Uuid>,
        limit: u32,
        prefix: &str,
    ) -> Result<(Vec<Uuid>, Option<Uuid>)> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        control.require_actor(actor)?;
        control.require_read(None)?;
        let mut params = scope(actor)?;
        params.extend([
            ("owner", owner_key(actor.owner())?.into_value()),
            ("provider", self.provider_instance_id.into_value()),
            ("after", after.into_value()),
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
        let mut ids: Vec<Uuid> = read.take(result).map_err(|_| ComputerError::Unavailable)?;
        let more = ids.len() > limit as usize;
        ids.truncate(limit as usize);
        let next = more.then(|| *ids.last().expect("nonzero limit"));
        control.require_read(None)?;
        Ok((ids, next))
    }
}
