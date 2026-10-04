//! Time persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TablePrefix,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("time")?, ModuleLayer::Optional)
        .ownership(vec![OwnershipClaim::TablePrefix(TablePrefix::new(
            "time_",
        )?)])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new(
            "identity",
        )?)])
        .build()
}
