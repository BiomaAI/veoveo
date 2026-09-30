use super::{Application, Result};
use veoveo_computers::{ComputerActor, api::*};

impl Application {
    pub async fn automation_grants(
        &self,
        actor: &ComputerActor,
        computer: veoveo_computers_contract::ComputerId,
    ) -> Result<AutomationGrantCollection> {
        Ok(self.store.list_automation_grants(actor, computer).await?)
    }
    pub async fn automation_grant(
        &self,
        actor: &ComputerActor,
        computer: veoveo_computers_contract::ComputerId,
        grant: veoveo_computers_contract::AutomationGrantId,
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
