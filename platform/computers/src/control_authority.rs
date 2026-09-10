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
        let (snapshot, session_expires_at) = tokio::time::timeout(Duration::from_secs(5), async {
            let snapshot = self.read_authority(actor.accepted()).await?;
            let expires = self.check_control_session(&snapshot).await?;
            Ok::<_, ComputerError>((snapshot, expires))
        })
        .await
        .map_err(|_| ComputerError::Unavailable)??;
        actor.check_admission()?;
        let expires_at = session_expires_at.map_or(actor.admission_expires_at(), |expires| {
            expires.min(actor.admission_expires_at())
        });
        let remaining = (expires_at - Utc::now())
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
    pub fn has_browser_session(&self) -> bool {
        self.snapshot
            .accepted
            .request_context
            .access_token
            .session_family
            .is_some()
    }
    pub fn require_attach(&self, computer: Uuid) -> Result<()> {
        self.check_fresh()?;
        crate::session_grants::require_attach(&self.snapshot, computer)
    }
    pub fn valid_until(&self) -> Instant {
        self.admission_deadline.min(self.snapshot.deadline)
    }
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
            || crate::api::COMPUTERS_URI.into(),
            crate::api::computer_uri,
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
