//! Store persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    AnalyzerName, DeclarationError, LaneExecution, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TableName,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("store")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Analyzer(AnalyzerName::new("platform_search")?),
            OwnershipClaim::Table(TableName::new("changefeed_checkpoint")?),
            OwnershipClaim::Table(TableName::new("platform_schema_migration")?),
            OwnershipClaim::Table(TableName::new("platform_downstream_migration")?),
            OwnershipClaim::Table(TableName::new("platform_module_installation")?),
            OwnershipClaim::Table(TableName::new("platform_module_lane")?),
            OwnershipClaim::Table(TableName::new("platform_module_migration")?),
        ])
        .lane(MigrationLane::empty())
        .execution(execution)
        .build()
}
