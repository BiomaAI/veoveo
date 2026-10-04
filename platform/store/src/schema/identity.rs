//! Identity persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TableName,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("identity")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Table(TableName::new("tenant")?),
            OwnershipClaim::Table(TableName::new("enterprise")?),
            OwnershipClaim::Table(TableName::new("principal")?),
            OwnershipClaim::Table(TableName::new("principal_group")?),
            OwnershipClaim::Table(TableName::new("membership")?),
            OwnershipClaim::Table(TableName::new("work_context")?),
            OwnershipClaim::Table(TableName::new("oauth_client")?),
        ])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("store")?)])
        .build()
}
