//! Agents persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, FunctionName, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer,
    ModuleName, ModuleSetup, OwnershipClaim, TableName, TablePrefix,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("agents")?, ModuleLayer::Optional)
        .ownership(vec![
            OwnershipClaim::TablePrefix(TablePrefix::new("agent_")?),
            OwnershipClaim::TablePrefix(TablePrefix::new("managed_agent_")?),
            OwnershipClaim::Table(TableName::new("managed_agent")?),
            OwnershipClaim::Table(TableName::new("agent")?),
            OwnershipClaim::Table(TableName::new("wake")?),
            OwnershipClaim::Function(FunctionName::new("fn::agent_catalog_authority")?),
            OwnershipClaim::Function(FunctionName::new("fn::agent_definition_editor")?),
            OwnershipClaim::Function(FunctionName::new("fn::agent_chat_revision")?),
            OwnershipClaim::Function(FunctionName::new("fn::managed_agent_editor")?),
            OwnershipClaim::Function(FunctionName::new("fn::managed_agent_enabled")?),
            OwnershipClaim::Function(FunctionName::new("fn::managed_agent_claim")?),
            OwnershipClaim::Function(FunctionName::new("fn::agent_consume_results")?),
        ])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("audit")?)])
        .build()
}
