use super::{Application, Result};
use uuid::Uuid;
use veoveo_computers::{ComputerActor, api::*};

impl Application {
    pub async fn automation_grants(
        &self,
        actor: &ComputerActor,
        computer: Uuid,
    ) -> Result<AutomationGrantCollection> {
        Ok(self.store.list_automation_grants(actor, computer).await?)
    }
    pub async fn automation_grant(
        &self,
        actor: &ComputerActor,
        computer: Uuid,
        grant: Uuid,
    ) -> Result<AutomationGrantResult> {
        Ok(self
            .store
            .get_automation_grant(actor, computer, grant)
            .await?
            .into())
    }
    pub async fn grant_automation(
        &self,
        actor: &ComputerActor,
        input: IssueAutomationGrantInput,
    ) -> Result<AutomationGrantResult> {
        Ok(self
            .store
            .issue_automation_grant(actor, &input)
            .await?
            .into())
    }
    pub async fn revoke_automation(
        &self,
        actor: &ComputerActor,
        input: RevokeAutomationGrantInput,
    ) -> Result<AutomationGrantResult> {
        Ok(self
            .store
            .revoke_automation_grant(actor, &input)
            .await?
            .into())
    }
}
