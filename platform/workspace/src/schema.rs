//! Workspace persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, FunctionName, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer,
    ModuleName, ModuleSetup, OwnershipClaim, TablePrefix,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("workspace")?, ModuleLayer::Optional)
        .ownership(vec![
            OwnershipClaim::TablePrefix(TablePrefix::new("workspace_")?),
            OwnershipClaim::Function(FunctionName::new("fn::workspace_authority")?),
            OwnershipClaim::Function(FunctionName::new("fn::workspace_member")?),
            OwnershipClaim::Function(FunctionName::new("fn::workspace_operation_owner")?),
        ])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("agents")?)])
        .build()
}
