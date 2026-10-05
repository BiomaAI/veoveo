//! Store persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    AnalyzerName, DeclarationError, LaneExecution, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TableName,
};

pub const CURRENT_SCHEMA: &str = include_str!("store/migrations/0000_current.surql");

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("store")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Analyzer(AnalyzerName::new("platform_search")?),
            OwnershipClaim::Table(TableName::new("changefeed_checkpoint")?),
            OwnershipClaim::Table(TableName::new("platform_module_installation")?),
            OwnershipClaim::Table(TableName::new("platform_module_lane")?),
            OwnershipClaim::Table(TableName::new("platform_module_migration")?),
        ])
        .lane(MigrationLane::new(vec![veoveo_modules::Migration::new(
            veoveo_modules::MigrationVersion::new(0),
            veoveo_modules::MigrationName::new("current")?,
            CURRENT_SCHEMA,
        )?])?)
        .execution(execution)
        .build()
}
