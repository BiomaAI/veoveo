//! Additive Optimization Task catalog lane; activation belongs to installation composition.
use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, Migration, MigrationLane, MigrationName,
    MigrationVersion, ModuleLayer, ModuleName, ModuleOwnership, ModuleSetup, OwnershipClaim,
    TableName,
};

pub fn ownership() -> Result<ModuleOwnership, DeclarationError> {
    ModuleOwnership::new(
        ModuleName::new("optimization")?,
        ModuleLayer::Optional,
        vec![OwnershipClaim::Table(TableName::new("optimization_task")?)],
    )
}

pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::from_ownership(ownership()?)
        .lane(MigrationLane::new(vec![
            Migration::new(
                MigrationVersion::new(0),
                MigrationName::new("task_catalog")?,
                include_str!("../migrations/0000_task_catalog.surql"),
            )?
            .with_requirements(vec![LaneRequirement::AtLeast {
                module: ModuleName::new("tasks")?,
                version: MigrationVersion::new(0),
            }])?,
        ])?)
        .execution(execution)
        .requires(vec![LaneRequirement::AtLeast {
            module: ModuleName::new("tasks")?,
            version: MigrationVersion::new(0),
        }])
        .build()
}
