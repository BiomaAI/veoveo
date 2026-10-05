//! Audit persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, FunctionName, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer,
    ModuleName, ModuleSetup, OwnershipClaim, TablePrefix,
};

pub const CURRENT_SCHEMA: &str = include_str!("audit/migrations/0000_current.surql");

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("audit")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::TablePrefix(TablePrefix::new("audit_")?),
            OwnershipClaim::Function(FunctionName::new("fn::append_audit")?),
            OwnershipClaim::Function(FunctionName::new("fn::append_audit_indexing")?),
        ])
        .lane(MigrationLane::new(vec![veoveo_modules::Migration::new(
            veoveo_modules::MigrationVersion::new(0),
            veoveo_modules::MigrationName::new("current")?,
            CURRENT_SCHEMA,
        )?])?)
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("tasks")?)])
        .build()
}
