//! Tasks persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TableName,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("tasks")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Table(TableName::new("task")?),
            OwnershipClaim::Table(TableName::new("task_input")?),
            OwnershipClaim::Table(TableName::new("task_idempotency")?),
            OwnershipClaim::Table(TableName::new("task_produced_artifact")?),
            OwnershipClaim::Table(TableName::new("task_used_artifact")?),
            OwnershipClaim::Table(TableName::new("provider_job")?),
            OwnershipClaim::Table(TableName::new("provider_event")?),
            OwnershipClaim::Table(TableName::new("domain_usage")?),
        ])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new(
            "artifacts",
        )?)])
        .build()
}
