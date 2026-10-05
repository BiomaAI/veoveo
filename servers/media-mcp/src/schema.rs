//! Media persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleOwnership, ModuleSetup, OwnershipClaim, TableName,
};

pub const CURRENT_SCHEMA: &str = include_str!("schema/migrations/0000_current.surql");

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::from_ownership(ownership()?)
        .lane(MigrationLane::new(vec![veoveo_modules::Migration::new(
            veoveo_modules::MigrationVersion::new(0),
            veoveo_modules::MigrationName::new("current")?,
            CURRENT_SCHEMA,
        )?])?)
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("tasks")?)])
        .build()
}

pub fn ownership() -> Result<ModuleOwnership, DeclarationError> {
    ModuleOwnership::new(
        ModuleName::new("media")?,
        ModuleLayer::Optional,
        vec![
            OwnershipClaim::Table(TableName::new("media_task_context")?),
            OwnershipClaim::Table(TableName::new("media_usage")?),
            OwnershipClaim::Table(TableName::new("media_task")?),
        ],
    )
}
