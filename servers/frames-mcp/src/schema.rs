//! Frames persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TableName,
};

pub const CURRENT_SCHEMA: &str = include_str!("schema/migrations/0000_current.surql");

pub const DEFINITION_WIRE_SCHEMA: &str =
    include_str!("schema/migrations/0001_definition_wire.surql");

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("frames")?, ModuleLayer::Optional)
        .ownership(vec![
            OwnershipClaim::Table(TableName::new("frame_world")?),
            OwnershipClaim::Table(TableName::new("frame_world_revision")?),
            OwnershipClaim::Table(TableName::new("coordinate_operation")?),
        ])
        .lane(MigrationLane::new(vec![
            veoveo_modules::Migration::new(
                veoveo_modules::MigrationVersion::new(0),
                veoveo_modules::MigrationName::new("current")?,
                CURRENT_SCHEMA,
            )?,
            veoveo_modules::Migration::new(
                veoveo_modules::MigrationVersion::new(1),
                veoveo_modules::MigrationName::new("definition_wire")?,
                DEFINITION_WIRE_SCHEMA,
            )?,
        ])?)
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("tasks")?)])
        .build()
}
