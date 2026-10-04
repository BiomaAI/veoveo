//! Knowledge persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    AnalyzerName, DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer,
    ModuleName, ModuleSetup, OwnershipClaim, TablePrefix,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("knowledge")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Analyzer(AnalyzerName::new("knowledge_text")?),
            OwnershipClaim::TablePrefix(TablePrefix::new("knowledge_")?),
        ])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("audit")?)])
        .build()
}
