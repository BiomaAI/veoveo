//! Uav persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TablePrefix,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("uav")?, ModuleLayer::Optional)
        .ownership(vec![OwnershipClaim::TablePrefix(TablePrefix::new("uav_")?)])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("agents")?)])
        .build()
}
