//! Gateway persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TableName, TablePrefix,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("gateway")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::TablePrefix(TablePrefix::new("gateway_")?),
            OwnershipClaim::Table(TableName::new("policy_revision")?),
            OwnershipClaim::Table(TableName::new("profile")?),
            OwnershipClaim::Table(TableName::new("profile_server")?),
            OwnershipClaim::Table(TableName::new("mcp_server")?),
            OwnershipClaim::Table(TableName::new("mcp_interaction")?),
        ])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new(
            "identity",
        )?)])
        .build()
}
