//! Current named lifecycle access, separate from retained ownership.
use crate::{
    ComputerActor, ComputerError, ComputersStore, Operation, Result,
    api::{Action, AutomationPermission},
    operation_reads::{OperationLookup, OperationParticipant},
};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use uuid::Uuid;

/// Short metadata/cancellation authority. Task attribution remains the actual
/// accepted actor even when the retained owner exercises current control.
pub struct OperationAccess {
    operation: Operation,
    deadline: Instant,
}
impl OperationAccess {
    pub fn operation(&self) -> Result<&Operation> {
        if Instant::now() >= self.deadline {
            return Err(ComputerError::Forbidden);
        }
        Ok(&self.operation)
    }
    pub fn valid_until(&self) -> Instant {
        self.deadline
    }
}

pub(crate) fn permission(action: Action) -> Result<AutomationPermission> {
    match action {
        Action::Start => Ok(AutomationPermission::Start),
        Action::Stop => Ok(AutomationPermission::Stop),
        Action::Create => Err(ComputerError::Forbidden),
    }
}

impl ComputersStore {
    pub async fn automation_operation_for_request(
        &self,
        actor: &ComputerActor,
        computer: veoveo_computers_contract::ComputerId,
        request: crate::api::RequestId,
        grant: crate::api::AutomationGrantId,
        action: Action,
    ) -> Result<Option<Operation>> {
        actor.check_admission()?;
        let mut read = self
            .query(
                include_str!(
                    "../queries/operation_authority/automation_operation_for_request.surql"
                ),
                vec![(
                    "request",
                    crate::operation_admission::request_record(actor.owner(), computer, request)?
                        .into_value(),
                )],
            )
            .await?;
        let Some(id) = read
            .take::<Option<Uuid>>(0)
            .map_err(|_| ComputerError::Unavailable)?
        else {
            return Ok(None);
        };
        let operation = self
            .automation_operation(actor, veoveo_types::TaskId::from_uuid(id))
            .await?;
        if operation.computer_id != computer
            || operation.action != action
            || operation.automation_grant_id != Some(grant)
        {
            return Err(ComputerError::RequestConflict);
        }
        Ok(Some(operation))
    }
    pub async fn authorize_operation_task(
        &self,
        actor: &ComputerActor,
        id: veoveo_types::TaskId,
        cancel: bool,
    ) -> Result<OperationAccess> {
        actor.check_admission()?;
        tokio::time::timeout(Duration::from_secs(5), async {
            let started = Instant::now();
            let control = self.control_authority(actor).await?;
            let mut deadline = control.valid_until().min(started + Duration::from_secs(5));
            let operation = match self
                .operation_lookup(actor.owner(), id, OperationParticipant::Owner)
                .await
            {
                Ok(lookup) => {
                    control.require_read(Some(lookup.computer))?;
                    if cancel {
                        control.require_action(lookup.action)?;
                    }
                    let computer = self.get(actor.owner(), lookup.computer).await?;
                    self.read_admitted_operation(actor.owner(), lookup, &computer.owner)
                        .await?
                }
                Err(ComputerError::NotFound) => {
                    let lookup = self
                        .operation_lookup(actor.owner(), id, OperationParticipant::Actor)
                        .await?;
                    control.require_read(Some(lookup.computer))?;
                    let authority = self.automation_operation_authority(actor, &lookup).await?;
                    control.require_same_revision(authority.control_revision())?;
                    deadline = deadline.min(authority.valid_until());
                    let operation = self
                        .read_admitted_operation(
                            actor.owner(),
                            lookup,
                            &authority.computer()?.owner,
                        )
                        .await?;
                    authority.computer()?;
                    operation
                }
                Err(error) => return Err(error),
            };
            control.require_actor(actor)?;
            let access = OperationAccess {
                operation,
                deadline,
            };
            access.operation()?;
            Ok(access)
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    /// The original caller may inspect its Task while its named action grant is
    /// current. This supplies no authority over another actor's lifecycle work.
    pub async fn automation_operation(
        &self,
        actor: &ComputerActor,
        id: veoveo_types::TaskId,
    ) -> Result<Operation> {
        actor.check_admission()?;
        tokio::time::timeout(Duration::from_secs(5), async {
            let lookup = self
                .operation_lookup(actor.owner(), id, OperationParticipant::Actor)
                .await?;
            let authority = self.automation_operation_authority(actor, &lookup).await?;
            let operation = self
                .read_admitted_operation(actor.owner(), lookup, &authority.computer()?.owner)
                .await?;
            authority.computer()?;
            actor.check_admission()?;
            Ok(operation)
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    async fn automation_operation_authority(
        &self,
        actor: &ComputerActor,
        lookup: &OperationLookup,
    ) -> Result<crate::automation_grants::AutomationAuthority> {
        self.authorize_automation_grant(
            actor,
            lookup.computer,
            lookup.grant.ok_or(ComputerError::NotFound)?,
            permission(lookup.action)?,
        )
        .await
    }

    pub async fn ensure_automation_operation_task(
        &self,
        actor: &ComputerActor,
        id: veoveo_types::TaskId,
    ) -> Result<Operation> {
        let operation = self.automation_operation(actor, id).await?;
        self.link_operation_task(operation).await
    }
}
