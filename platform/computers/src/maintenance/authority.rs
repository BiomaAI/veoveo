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
    /// Accepted maintenance outlives its admission token. Each new step still
    /// requires current directory, Work Context and named action policy.
    pub(super) async fn authorize_maintenance(
        &self,
        operation: &super::MaintenanceOperation,
    ) -> Result<crate::current_authority::ExecutionPermit> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let snapshot = self.read_authority(&operation.execution_authority).await?;
            let deadline = snapshot.deadline - Duration::from_secs(25);
            let decision = snapshot.decision(
                GatewayAction::ToolsCall,
                &target(),
                &TraceId::new(operation.operation_id.to_string()).expect("UUID trace"),
            );
            if deadline <= std::time::Instant::now() {
                return Err(ComputerError::Unavailable);
            }
            if !snapshot
                .membership
                .allows(WorkContextMembershipLevel::Contributor)
                || decision.effect != PolicyEffect::Allow
            {
                return Err(ComputerError::Forbidden);
            }
            Ok(crate::current_authority::ExecutionPermit {
                evidence: crate::ExecutionDecision {
                    control_revision: snapshot.control_revision,
                    control_sha256: snapshot.control_sha256,
                    checked_at: snapshot.checked_at,
                    valid_until: snapshot.checked_at + chrono::TimeDelta::seconds(5),
                    decision,
                },
                deadline,
                revision_record: snapshot.revision_record,
                tenant: snapshot.tenant,
                source: snapshot.source,
                actor: snapshot.actor,
            })
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

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
