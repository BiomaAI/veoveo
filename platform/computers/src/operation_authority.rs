//! Current named lifecycle access, separate from retained ownership.
use crate::{
    ComputerActor, ComputerError, ComputersStore, Operation, Result,
    api::{Action, AutomationPermission},
    identity::permits,
};
use std::time::{Duration, Instant};
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
    pub async fn authorize_operation_task(
        &self,
        actor: &ComputerActor,
        id: Uuid,
        cancel: bool,
    ) -> Result<OperationAccess> {
        let started = Instant::now();
        let operation = self.read_operation(id).await?;
        let control = self.control_authority(actor).await?;
        control.require_read(Some(operation.computer_id))?;
        let mut deadline = control.valid_until().min(started + Duration::from_secs(5));
        if permits(&operation.owner, actor.owner()).is_ok() {
            self.get(actor.owner(), operation.computer_id).await?;
            if cancel {
                control.require_action(operation.action)?;
            }
        } else {
            let authority = self
                .automation_operation_authority(actor, &operation)
                .await?;
            deadline = deadline.min(authority.valid_until());
        }
        let access = OperationAccess {
            operation,
            deadline,
        };
        access.operation()?;
        Ok(access)
    }

    /// The original caller may inspect its Task while its named action grant is
    /// current. This supplies no authority over another actor's lifecycle work.
    pub async fn automation_operation(&self, actor: &ComputerActor, id: Uuid) -> Result<Operation> {
        let operation = self.read_operation(id).await?;
        self.automation_operation_authority(actor, &operation)
            .await?;
        Ok(operation)
    }

    async fn automation_operation_authority(
        &self,
        actor: &ComputerActor,
        operation: &Operation,
    ) -> Result<crate::automation_grants::AutomationAuthority> {
        permits(&operation.actor, actor.owner())?;
        let grant = operation
            .automation_grant_id
            .ok_or(ComputerError::NotFound)?;
        let authority = self
            .authorize_automation_grant(
                actor,
                operation.computer_id,
                grant,
                permission(operation.action)?,
            )
            .await?;
        if authority.computer()?.owner != operation.owner {
            return Err(ComputerError::Forbidden);
        }
        Ok(authority)
    }

    pub async fn ensure_automation_operation_task(
        &self,
        actor: &ComputerActor,
        id: Uuid,
    ) -> Result<Operation> {
        let operation = self.automation_operation(actor, id).await?;
        self.link_operation_task(operation).await
    }
}
