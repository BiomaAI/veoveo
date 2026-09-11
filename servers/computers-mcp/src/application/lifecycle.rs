//! Named lifecycle projection over the same durable Computer journal.
use super::{Application, ApplicationError, Result};
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputerError, Operation,
    api::{Action, AutomationPermission},
};

impl Application {
    pub(super) async fn granted_lifecycle(
        &self,
        actor: &ComputerActor,
        computer: Uuid,
        request: Uuid,
        grant: Uuid,
        action: Action,
    ) -> Result<Operation> {
        let permission = match action {
            Action::Start => AutomationPermission::Start,
            Action::Stop => AutomationPermission::Stop,
            Action::Create => return Err(ComputerError::Forbidden.into()),
        };
        let authority = self
            .store
            .authorize_automation_grant(actor, computer, grant, permission)
            .await?;
        if let Some(prior) = self
            .store
            .automation_operation_for_request(actor, computer, request, grant, action)
            .await?
        {
            return Ok(self
                .store
                .ensure_automation_operation_task(actor, prior.operation_id)
                .await?);
        }
        let selected = authority.computer()?;
        if !self.admits(action)
            || !self
                .templates
                .contains(&selected.template_id, &selected.template_fingerprint)
        {
            return Err(ApplicationError::Unavailable);
        }
        let operation = self
            .store
            .queue_automation_operation(actor, authority, request, action)
            .await?;
        Ok(self
            .store
            .ensure_automation_operation_task(actor, operation.operation_id)
            .await?)
    }
}
