//! Workspace persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, FunctionName, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer,
    ModuleName, ModuleSetup, OwnershipClaim, TablePrefix,
};

pub const CURRENT_SCHEMA: &str = include_str!("schema/migrations/0000_current.surql");

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("workspace")?, ModuleLayer::Optional)
        .ownership(vec![
            OwnershipClaim::TablePrefix(TablePrefix::new("workspace_")?),
            OwnershipClaim::Function(FunctionName::new("fn::workspace_authority")?),
            OwnershipClaim::Function(FunctionName::new("fn::workspace_member")?),
            OwnershipClaim::Function(FunctionName::new("fn::workspace_operation_owner")?),
        ])
        .lane(MigrationLane::new(vec![
            veoveo_modules::Migration::new(
                veoveo_modules::MigrationVersion::new(0),
                veoveo_modules::MigrationName::new("current")?,
                CURRENT_SCHEMA,
            )?
            .with_requirements(vec![
                LaneRequirement::AtLeast {
                    module: ModuleName::new("identity")?,
                    version: veoveo_modules::MigrationVersion::new(0),
                },
                LaneRequirement::AtLeast {
                    module: ModuleName::new("agents")?,
                    version: veoveo_modules::MigrationVersion::new(0),
                },
            ])?,
        ])?)
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("agents")?)])
        .build()
}
