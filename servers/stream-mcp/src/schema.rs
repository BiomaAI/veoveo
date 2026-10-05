//! Stream-owned Task lookup declaration, independent of execution runtimes.
use veoveo_modules::*;
pub fn ownership() -> Result<ModuleOwnership, DeclarationError> {
    ModuleOwnership::new(
        ModuleName::new("stream")?,
        ModuleLayer::Optional,
        vec![OwnershipClaim::Table(TableName::new("stream_run")?)],
    )
}
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::from_ownership(ownership()?)
        .lane(MigrationLane::new(vec![
            Migration::new(
                MigrationVersion::new(0),
                MigrationName::new("task_lookup")?,
                include_str!("schema/migrations/0000_current.surql"),
            )?
            .with_requirements(vec![
                LaneRequirement::AtLeast {
                    module: ModuleName::new("tasks")?,
                    version: MigrationVersion::new(0),
                },
                LaneRequirement::AtLeast {
                    module: ModuleName::new("artifacts")?,
                    version: MigrationVersion::new(0),
                },
            ])?,
        ])?)
        .requires(vec![
            LaneRequirement::AtLeast {
                module: ModuleName::new("tasks")?,
                version: MigrationVersion::new(0),
            },
            LaneRequirement::AtLeast {
                module: ModuleName::new("artifacts")?,
                version: MigrationVersion::new(0),
            },
        ])
        .execution(execution)
        .build()
}
