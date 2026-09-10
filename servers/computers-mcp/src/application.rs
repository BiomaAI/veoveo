//! Shared commands and projections for MCP and the Console HTTP adapter.
use crate::Templates;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use uuid::Uuid;
use veoveo_computers::{
    Computer, ComputerActor, ComputerError, ComputersStore, ControlAuthority, Operation,
    Reservation, api::*,
};
use veoveo_task_runtime::TaskRuntime;

#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] ComputerError),
    #[error("Computers configuration is invalid")]
    Configuration,
    #[error("Computers capacity is not configured")]
    SetupRequired,
    #[error("Computer capacity is unavailable")]
    Unavailable,
}
pub type Result<T> = std::result::Result<T, ApplicationError>;

/// A local provider probe supplies availability, never user authorization.
#[derive(Clone, Copy)]
pub struct CapacityHealth {
    pub availability: CapacityAvailability,
    pub observed_at: Instant,
}
pub struct Application {
    pub(crate) subscription_slots: std::sync::Arc<tokio::sync::Semaphore>,
    pub(crate) store: ComputersStore,
    pub(crate) tasks: TaskRuntime,
    pub(crate) templates: Templates,
    pub(crate) runtime: crate::RuntimeAccess,
    pub(crate) terminal_slots: std::sync::Arc<tokio::sync::Semaphore>,
    health: watch::Receiver<CapacityHealth>,
}
impl Application {
    pub fn task_runtime(&self) -> &TaskRuntime {
        &self.tasks
    }
    pub(crate) fn capacity_health(&self) -> watch::Receiver<CapacityHealth> {
        self.health.clone()
    }
    pub fn new(
        store: ComputersStore,
        tasks: TaskRuntime,
        templates: Templates,
        health: watch::Receiver<CapacityHealth>,
        runtime: crate::RuntimeAccess,
    ) -> Result<Self> {
        if tasks.server() != "computers" {
            return Err(ApplicationError::Configuration);
        }
        Ok(Self {
            subscription_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(64)),
            store,
            tasks,
            templates,
            health,
            runtime,
            terminal_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(128)),
        })
    }
    pub fn availability(&self) -> CapacityAvailability {
        let health = *self.health.borrow();
        if health.availability == CapacityAvailability::SetupRequired {
            return health.availability;
        }
        if health.observed_at > Instant::now()
            || health.observed_at.elapsed() > Duration::from_secs(15)
        {
            return CapacityAvailability::ComputeUnavailable;
        }
        health.availability
    }
    fn admits(&self, action: Action) -> bool {
        match self.availability() {
            CapacityAvailability::Available => true,
            CapacityAvailability::StorageUnavailable => action == Action::Stop,
            _ => false,
        }
    }
    pub async fn snapshot(
        &self,
        actor: &ComputerActor,
        after: Option<Uuid>,
    ) -> Result<ComputerSnapshot> {
        let authority = self.store.control_authority(actor).await?;
        authority.require_read(None)?;
        let page = self.store.list(actor.owner(), after, 100).await?;
        let (capacity, room) = match self.store.capacity_for(actor.owner()).await {
            Ok((c, room)) => (
                Some(ComputerLimits {
                    per_owner: c.per_owner,
                    per_tenant: c.per_tenant,
                    provider: c.provider,
                }),
                room,
            ),
            Err(_) if self.availability() == CapacityAvailability::SetupRequired => (None, false),
            Err(e) => return Err(e.into()),
        };
        let access = self.browser_access(&authority).await?;
        let computers = page
            .computers
            .iter()
            .map(|c| self.view(c, &authority, access))
            .collect();
        authority.require_read(None)?;
        Ok(ComputerSnapshot {
            availability: if self.availability() == CapacityAvailability::Available && !room {
                CapacityAvailability::Exhausted
            } else {
                self.availability()
            },
            template: self.templates.default().map(|t| t.view()),
            limits: capacity,
            can_create: room
                && self.templates.default().is_some()
                && self.admits(Action::Create)
                && authority.allows_action(Action::Create),
            computers,
            next_cursor: page.next_cursor,
        })
    }
    pub async fn computer(&self, actor: &ComputerActor, id: Uuid) -> Result<ComputerView> {
        let authority = self.store.control_authority(actor).await?;
        authority.require_read(Some(id))?;
        let computer = self.store.get(actor.owner(), id).await?;
        let access = self.browser_access(&authority).await?;
        authority.require_read(Some(id))?;
        Ok(self.view(&computer, &authority, access))
    }
    pub async fn operation(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
        operation_id: Uuid,
    ) -> Result<Operation> {
        let authority = self.store.control_authority(actor).await?;
        authority.require_read(Some(computer_id))?;
        let operation = self.store.operation(actor.owner(), operation_id).await?;
        if operation.computer_id != computer_id {
            return Err(ComputerError::NotFound.into());
        }
        authority.require_read(Some(computer_id))?;
        Ok(operation)
    }
    async fn browser_access(&self, authority: &ControlAuthority) -> Result<bool> {
        if !authority.has_browser_session() || self.runtime.current().is_err() {
            return Ok(false);
        }
        Ok(self.store.session_grant_policy().await?.max_grants > 0)
    }
    fn view(
        &self,
        computer: &Computer,
        authority: &ControlAuthority,
        access: bool,
    ) -> ComputerView {
        let unfenced = computer.active_operation.is_none();
        let template = computer.provider_instance_id == self.store.provider_instance_id()
            && self
                .templates
                .contains(&computer.template_id, &computer.template_fingerprint);
        let admitted = |a| template && unfenced && self.admits(a) && authority.allows_action(a);
        ComputerView {
            computer_id: computer.computer_id,
            template_id: computer.template_id.clone(),
            phase: computer.phase,
            busy: !unfenced,
            can_create: computer.phase == ComputerPhase::Reserved && admitted(Action::Create),
            can_start: computer.phase == ComputerPhase::Stopped && admitted(Action::Start),
            can_stop: computer.phase == ComputerPhase::Ready && admitted(Action::Stop),
            // Deletion becomes actionable with retained purge.
            can_delete: false,
            can_connect: access
                && template
                && unfenced
                && computer.phase == ComputerPhase::Ready
                && self.runtime.current().is_ok()
                && authority.require_attach(computer.computer_id).is_ok(),
            active_task_id: computer.active_operation,
            created_at: computer.created_at,
            updated_at: computer.updated_at,
        }
    }
    pub async fn create(&self, actor: ComputerActor, request: CreateInput) -> Result<Operation> {
        if let Some(computer_id) = request.computer_id {
            return self
                .lifecycle(
                    actor,
                    LifecycleInput {
                        computer_id,
                        request_id: request.request_id,
                    },
                    Action::Create,
                )
                .await;
        }
        let authority = self.store.control_authority(&actor).await?;
        authority.require_action(Action::Create)?;
        let prior = self
            .store
            .reserved_for_request(actor.owner(), request.request_id)
            .await?;
        let computer = if let Some(prior) = prior {
            prior
        } else {
            let selected = self
                .templates
                .default()
                .ok_or(ApplicationError::SetupRequired)?;
            if !self.admits(Action::Create) {
                return Err(ApplicationError::Unavailable);
            }
            match self
                .store
                .reserve(
                    actor.owner(),
                    &Reservation {
                        request_id: request.request_id,
                        template_id: selected.id.clone(),
                        template_fingerprint: selected.runtime.fingerprint(),
                    },
                )
                .await
            {
                Ok(computer) => computer,
                Err(ComputerError::RequestConflict) => self
                    .store
                    .reserved_for_request(actor.owner(), request.request_id)
                    .await?
                    .ok_or(ApplicationError::Unavailable)?,
                Err(e) => return Err(e.into()),
            }
        };
        authority.require_action(Action::Create)?;
        self.accept(
            actor,
            computer.computer_id,
            request.request_id,
            Action::Create,
            &authority,
        )
        .await
    }
    pub async fn lifecycle(
        &self,
        actor: ComputerActor,
        request: LifecycleInput,
        action: Action,
    ) -> Result<Operation> {
        let authority = self.store.control_authority(&actor).await?;
        authority.require_action(action)?;
        self.accept(
            actor,
            request.computer_id,
            request.request_id,
            action,
            &authority,
        )
        .await
    }
    async fn accept(
        &self,
        actor: ComputerActor,
        computer: Uuid,
        request: Uuid,
        action: Action,
        authority: &ControlAuthority,
    ) -> Result<Operation> {
        let owner = actor.owner().clone();
        if let Some(prior) = self
            .store
            .operation_for_request(&owner, computer, request, action)
            .await?
        {
            authority.require_action(action)?;
            return Ok(self
                .store
                .ensure_operation_task(&owner, prior.operation_id)
                .await?);
        }
        if !self.admits(action) {
            return Err(ApplicationError::Unavailable);
        }
        let selected = self.store.get(&owner, computer).await?;
        if selected.provider_instance_id != self.store.provider_instance_id()
            || !self
                .templates
                .contains(&selected.template_id, &selected.template_fingerprint)
        {
            return Err(ApplicationError::Unavailable);
        }
        authority.require_action(action)?;
        let operation = self
            .store
            .queue_operation(actor, computer, request, action)
            .await?;
        Ok(self
            .store
            .ensure_operation_task(&owner, operation.operation_id)
            .await?)
    }
}
