//! Frames persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TableName,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("frames")?, ModuleLayer::Optional)
        .ownership(vec![
            OwnershipClaim::Table(TableName::new("frame_world")?),
            OwnershipClaim::Table(TableName::new("frame_world_revision")?),
            OwnershipClaim::Table(TableName::new("coordinate_operation")?),
            OwnershipClaim::Table(TableName::new("task_used_frame")?),
        ])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("tasks")?)])
        .build()
}
