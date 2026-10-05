//! Uav persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleOwnership, ModuleSetup, OwnershipClaim, TablePrefix,
};

pub const CURRENT_SCHEMA: &str = concat!(
    include_str!("schema/migrations/0000_current.surql"),
    include_str!("schema/migrations/0000_task_catalog.surql")
);

pub fn ownership() -> Result<ModuleOwnership, DeclarationError> {
    ModuleOwnership::new(
        ModuleName::new("uav")?,
        ModuleLayer::Optional,
        vec![OwnershipClaim::TablePrefix(TablePrefix::new("uav_")?)],
    )
}

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::from_ownership(ownership()?)
        .lane(MigrationLane::new(vec![
            veoveo_modules::Migration::new(
                veoveo_modules::MigrationVersion::new(0),
                veoveo_modules::MigrationName::new("current")?,
                CURRENT_SCHEMA,
            )?
            .with_requirements(vec![LaneRequirement::AtLeast {
                module: ModuleName::new("agents")?,
                version: veoveo_modules::MigrationVersion::new(0),
            }])?,
        ])?)
        .execution(execution)
        .requires(vec![
            LaneRequirement::Satisfied(ModuleName::new("agents")?),
            LaneRequirement::AtLeast {
                module: ModuleName::new("tasks")?,
                version: veoveo_modules::MigrationVersion::new(0),
            },
        ])
        .build()
}
