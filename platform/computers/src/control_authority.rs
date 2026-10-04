//! Request-scoped authority for public projections. It grants no dispatch ticket.
use crate::{ComputerActor, ComputerError, ComputersStore, Result, api::Action};
use chrono::Utc;
use std::time::{Duration, Instant};
use uuid::Uuid;
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{PolicyEffect, PolicyTarget, ServerSlug, TraceId};
use veoveo_types::ResourceUri;
use veoveo_types::WorkContextMembershipLevel;

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
    pub(crate) fn require_same_revision(
        &self,
        revision: &surrealdb::types::RecordId,
    ) -> Result<()> {
        self.check_fresh()?;
        if &self.snapshot.revision_record != revision {
            return Err(ComputerError::PolicyConflict);
        }
        Ok(())
    }
    pub(crate) fn read_bindings(&self) -> Result<Vec<(&'static str, surrealdb::types::Value)>> {
        use surrealdb::types::SurrealValue;
        self.check_fresh()?;
        let family = self
            .snapshot
            .accepted
            .request_context
            .access_token
            .session_family
            .as_ref()
            .map(|id| {
                id.as_str()
                    .parse::<Uuid>()
                    .map(veoveo_platform_store::gateway_refresh_family_record_id)
            })
            .transpose()
            .map_err(|_| ComputerError::Forbidden)?;
        let remaining = self.valid_until().saturating_duration_since(Instant::now());
        let expires = Utc::now()
            + chrono::TimeDelta::from_std(remaining).map_err(|_| ComputerError::Unavailable)?;
        let mut params = crate::session_grants::authority::bindings(&self.snapshot);
        params.extend([
            ("family", family.into_value()),
            ("authority_expires_at", expires.into_value()),
        ]);
        Ok(params)
    }
    pub(crate) fn require_actor(&self, actor: &ComputerActor) -> Result<()> {
        self.check_fresh()?;
        actor.check_admission()?;
        if crate::identity::digest(&self.snapshot.accepted)?
            != crate::identity::digest(actor.accepted())?
        {
            return Err(ComputerError::Forbidden);
        }
        Ok(())
    }
    pub fn has_browser_session(&self) -> bool {
        self.snapshot
            .accepted
            .request_context
            .access_token
            .session_family
            .is_some()
    }
    pub fn require_attach(&self, computer: veoveo_computers_contract::ComputerId) -> Result<()> {
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
        self.require_call(&crate::current_authority::execution_target(action))
    }
    pub fn require_update_template(&self) -> Result<()> {
        if self.snapshot.deadline - Duration::from_secs(25) <= Instant::now() {
            return Err(ComputerError::Unavailable);
        }
        self.require_call(&crate::maintenance::target())
    }
    pub fn require_resume_update(&self) -> Result<()> {
        if self.snapshot.deadline - Duration::from_secs(25) <= Instant::now() {
            return Err(ComputerError::Unavailable);
        }
        self.require_call(&crate::maintenance::resume_target())
    }
    fn require_call(&self, target: &PolicyTarget) -> Result<()> {
        self.check_fresh()?;
        if !self
            .snapshot
            .membership
            .allows(WorkContextMembershipLevel::Contributor)
            || self
                .snapshot
                .decision(GatewayAction::ToolsCall, target, &self.trace)
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
    pub fn allows_file_transfer(&self) -> bool {
        self.snapshot.accepted.actor.id == self.snapshot.accepted.request_context.principal.id
            && self.require_call(&crate::files::target()).is_ok()
    }
    /// Policy for the canonical collection or exact Computer resource; ownership
    /// and retained labels are independently enforced by the domain read.
    pub fn require_read(
        &self,
        computer: Option<veoveo_computers_contract::ComputerId>,
    ) -> Result<()> {
        self.check_fresh()?;
        let uri = computer.map_or_else(
            || crate::api::ComputerResource::Collection(None).to_uri(),
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
