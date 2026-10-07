//! Knowledge persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    AnalyzerName, DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer,
    ModuleName, ModuleSetup, OwnershipClaim, TablePrefix,
};

pub const CURRENT_SCHEMA: &str = include_str!("knowledge/migrations/0000_current.surql");

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("knowledge")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Analyzer(AnalyzerName::new("knowledge_text")?),
            OwnershipClaim::TablePrefix(TablePrefix::new("knowledge_")?),
        ])
        .lane(MigrationLane::new(vec![
            veoveo_modules::Migration::new(
                veoveo_modules::MigrationVersion::new(0),
                veoveo_modules::MigrationName::new("current")?,
                CURRENT_SCHEMA,
            )?,
            veoveo_modules::Migration::new(
                veoveo_modules::MigrationVersion::new(1),
                veoveo_modules::MigrationName::new("observation_policy")?,
                include_str!("knowledge/migrations/0001_observation_policy.surql"),
            )?,
        ])?)
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("audit")?)])
        .build()
}
