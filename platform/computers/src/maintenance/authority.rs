use crate::{
    ComputerActor, ComputerError, ComputersStore, Result, authority_snapshot::AuthoritySnapshot,
};
use std::time::Duration;
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayAction, LocalToolName, PolicyEffect, PolicyTarget, ServerSlug, TraceId,
    WorkContextMembershipLevel,
};

pub(super) fn target() -> PolicyTarget {
    PolicyTarget::Tool {
        server: ServerSlug::new("computers").expect("static server"),
        tool: LocalToolName::new("update_template").expect("static tool"),
    }
}
impl ComputersStore {
    pub(super) async fn maintenance_admission_authority(
        &self,
        actor: &ComputerActor,
    ) -> Result<AuthoritySnapshot> {
        actor.check_admission()?;
        let snapshot = tokio::time::timeout(Duration::from_secs(5), async {
            let snapshot = self.read_authority(actor.accepted()).await?;
            self.check_control_session(&snapshot).await?;
            snapshot.check_fresh()?;
            if !snapshot
                .membership
                .allows(WorkContextMembershipLevel::Contributor)
                || snapshot
                    .decision(
                        GatewayAction::ToolsCall,
                        &target(),
                        &TraceId::new(Uuid::now_v7().to_string()).expect("UUID trace"),
                    )
                    .effect
                    != PolicyEffect::Allow
            {
                return Err(ComputerError::Forbidden);
            }
            Ok(snapshot)
        })
        .await
        .map_err(|_| ComputerError::Unavailable)??;
        actor.check_admission()?;
        Ok(snapshot)
    }
}
