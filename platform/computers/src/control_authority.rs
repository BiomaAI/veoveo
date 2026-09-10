//! Request-scoped authority for public projections. It grants no dispatch ticket.
use crate::{ComputerActor, ComputerError, ComputersStore, Result, api::Action};
use chrono::Utc;
use std::time::{Duration, Instant};
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayAction, PolicyEffect, PolicyTarget, ResourceUri, ServerSlug, TraceId,
    WorkContextMembershipLevel,
};

pub struct ControlAuthority {
    snapshot: crate::authority_snapshot::AuthoritySnapshot,
    admission_deadline: Instant,
    trace: TraceId,
}
impl ComputersStore {
    /// One current catalog/directory read serves all action flags in one response.
    /// A new request must obtain a new snapshot. The worker still rechecks dispatch.
    pub async fn control_authority(&self, actor: &ComputerActor) -> Result<ControlAuthority> {
        actor.check_admission()?;
        let snapshot = tokio::time::timeout(
            Duration::from_secs(5),
            self.read_authority(actor.accepted()),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)??;
        actor.check_admission()?;
        let remaining = (actor.admission_expires_at() - Utc::now())
            .to_std()
            .map_err(|_| ComputerError::Forbidden)?;
        Ok(ControlAuthority {
            snapshot,
            admission_deadline: Instant::now() + remaining,
            trace: TraceId::new(Uuid::now_v7().to_string()).expect("UUID trace"),
        })
    }
}
impl ControlAuthority {
    fn check_fresh(&self) -> Result<()> {
        if self.admission_deadline <= Instant::now() {
            return Err(ComputerError::Forbidden);
        }
        self.snapshot.check_fresh()
    }
    pub fn require_action(&self, action: Action) -> Result<()> {
        self.check_fresh()?;
        if !self
            .snapshot
            .membership
            .allows(WorkContextMembershipLevel::Contributor)
            || self
                .snapshot
                .decision(
                    GatewayAction::ToolsCall,
                    &crate::current_authority::execution_target(action),
                    &self.trace,
                )
                .effect
                != PolicyEffect::Allow
        {
            return Err(ComputerError::Forbidden);
        }
        Ok(())
    }
    pub fn allows_action(&self, action: Action) -> bool {
        self.require_action(action).is_ok()
    }
    /// Policy for the canonical collection or exact Computer resource; ownership
    /// and retained labels are independently enforced by the domain read.
    pub fn require_read(&self, computer: Option<Uuid>) -> Result<()> {
        self.check_fresh()?;
        let uri = computer.map_or_else(
            || "computer://computers".into(),
            |id| format!("computer://computers/{id}"),
        );
        let target = PolicyTarget::Resource {
            server: ServerSlug::new("computers").expect("static server"),
            uri: ResourceUri::new(uri).map_err(|_| ComputerError::InvalidInput)?,
        };
        if self
            .snapshot
            .decision(GatewayAction::ResourcesRead, &target, &self.trace)
            .effect
            != PolicyEffect::Allow
        {
            return Err(ComputerError::Forbidden);
        }
        Ok(())
    }
}
