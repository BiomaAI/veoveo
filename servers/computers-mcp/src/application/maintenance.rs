use super::*;
use veoveo_computers::maintenance::{
    MaintenanceOperation, MaintenanceRecovery, MaintenanceStage, MaintenanceTarget,
};

impl Application {
    pub fn with_maintenance(mut self, profiles: crate::MaintenanceProfiles) -> Result<Self> {
        if !profiles.matches_catalog(&self.templates.runtimes()) {
            return Err(ApplicationError::Configuration);
        }
        self.maintenance = Some(profiles);
        Ok(self)
    }

    pub async fn maintenance_state(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
    ) -> Result<MaintenanceState> {
        let authority = self.store.control_authority(actor).await?;
        authority.require_read(Some(computer_id))?;
        let computer = self.store.get(actor.owner(), computer_id).await?;
        let active = match computer.active_operation {
            Some(id) => match self.store.maintenance(actor.owner(), id).await {
                Ok(operation) => Some(view(&operation)),
                Err(ComputerError::NotFound) => None,
                Err(error) => return Err(error.into()),
            },
            None => None,
        };
        let targets: Vec<_> = self
            .templates
            .iter()
            .filter(|target| {
                computer.provider_instance_id == self.store.provider_instance_id()
                    && self
                        .templates
                        .contains(&computer.template_id, &computer.template_fingerprint)
                    && self.maintenance.as_ref().is_some_and(|profiles| {
                        profiles.admits(
                            &computer.template_fingerprint,
                            &target.runtime.fingerprint(),
                        )
                    })
                    && authority.require_update_template().is_ok()
            })
            .map(|target| target.view())
            .collect();
        let can_update = !targets.is_empty()
            && active.is_none()
            && self.availability() == CapacityAvailability::Available
            && self
                .store
                .maintenance_available(actor.owner(), computer_id)
                .await?
            && authority.require_update_template().is_ok();
        authority.require_read(Some(computer_id))?;
        Ok(MaintenanceState {
            computer_id,
            targets,
            can_update,
            active,
        })
    }

    pub async fn maintenance_operation(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
        task_id: Uuid,
    ) -> Result<MaintenanceView> {
        let authority = self.store.control_authority(actor).await?;
        authority.require_read(Some(computer_id))?;
        let operation = self.store.maintenance(actor.owner(), task_id).await?;
        if operation.computer_id != computer_id {
            return Err(ComputerError::NotFound.into());
        }
        authority.require_read(Some(computer_id))?;
        Ok(view(&operation))
    }

    pub async fn update_template(
        &self,
        actor: &ComputerActor,
        request: UpdateTemplateInput,
    ) -> Result<MaintenanceOperation> {
        let authority = self.store.control_authority(actor).await?;
        authority.require_update_template()?;
        // A reply can be lost across a configuration change. Resolve the original
        // selection before requiring current capacity or looking at the default.
        if let Some(prior) = self
            .store
            .maintenance_for_request(actor.owner(), request.computer_id, request.request_id)
            .await?
        {
            if request
                .template_id
                .as_ref()
                .is_some_and(|id| *id != prior.target.template_id)
            {
                return Err(ComputerError::RequestConflict.into());
            }
            authority.require_update_template()?;
            return Ok(self
                .store
                .ensure_maintenance_task(actor.owner(), prior.operation_id)
                .await?);
        }
        let computer = self.store.get(actor.owner(), request.computer_id).await?;
        let selected = self
            .templates
            .select(request.template_id.as_deref())
            .ok_or(ComputerError::InvalidInput)?;
        if self.availability() != CapacityAvailability::Available
            || computer.provider_instance_id != self.store.provider_instance_id()
            || !self
                .templates
                .contains(&computer.template_id, &computer.template_fingerprint)
            || !self.maintenance.as_ref().is_some_and(|profiles| {
                profiles.admits(
                    &computer.template_fingerprint,
                    &selected.runtime.fingerprint(),
                )
            })
        {
            return Err(ApplicationError::Unavailable);
        }
        authority.require_update_template()?;
        match self
            .store
            .queue_maintenance(
                actor,
                request.computer_id,
                request.request_id,
                &MaintenanceTarget {
                    template_id: selected.id.clone(),
                    template_fingerprint: selected.runtime.fingerprint(),
                },
            )
            .await
        {
            Ok(operation) => Ok(operation),
            Err(ComputerError::RequestConflict) => {
                let prior = self
                    .store
                    .maintenance_for_request(actor.owner(), request.computer_id, request.request_id)
                    .await?
                    .ok_or(ComputerError::RequestConflict)?;
                if request
                    .template_id
                    .as_ref()
                    .is_some_and(|id| *id != prior.target.template_id)
                {
                    return Err(ComputerError::RequestConflict.into());
                }
                authority.require_update_template()?;
                Ok(self
                    .store
                    .ensure_maintenance_task(actor.owner(), prior.operation_id)
                    .await?)
            }
            Err(error) => Err(error.into()),
        }
    }
}

pub(crate) fn view(operation: &MaintenanceOperation) -> MaintenanceView {
    MaintenanceView {
        computer_id: operation.computer_id,
        task_id: operation.operation_id,
        source_template_id: operation.source_template_id.clone(),
        target_template_id: operation.target.template_id.clone(),
        phase: match operation.stage {
            MaintenanceStage::Queued => MaintenancePhase::Queued,
            MaintenanceStage::Stopping => MaintenancePhase::Stopping,
            MaintenanceStage::Capturing => MaintenancePhase::SavingPolicy,
            MaintenanceStage::Retiring | MaintenanceStage::Transferring => {
                MaintenancePhase::Replacing
            }
            MaintenanceStage::Creating => MaintenancePhase::Starting,
            MaintenanceStage::Restoring => MaintenancePhase::RestoringPolicy,
            MaintenanceStage::Adopting => MaintenancePhase::Verifying,
            MaintenanceStage::Succeeded => MaintenancePhase::Succeeded,
            MaintenanceStage::Cancelled => MaintenancePhase::Cancelled,
            MaintenanceStage::RecoveryRequired => MaintenancePhase::RecoveryRequired,
        },
        recovery: operation.recovery().map(|reason| match reason {
            MaintenanceRecovery::BudgetExhausted => {
                MaintenanceRecoveryReason::ObservationBudgetExhausted
            }
            MaintenanceRecovery::AuthorityDenied => MaintenanceRecoveryReason::AuthorityDenied,
            MaintenanceRecovery::CancellationRequested => {
                MaintenanceRecoveryReason::CancellationRequested
            }
            MaintenanceRecovery::InvalidCheckpoint => {
                MaintenanceRecoveryReason::CheckpointUnavailable
            }
        }),
        created_at: operation.created_at,
        updated_at: operation.updated_at,
    }
}
