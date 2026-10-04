//! Artifacts persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, FunctionName, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer,
    ModuleName, ModuleSetup, OwnershipClaim, TableName, TablePrefix,
};

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("artifacts")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::TablePrefix(TablePrefix::new("artifact_")?),
            OwnershipClaim::Table(TableName::new("share_link")?),
            OwnershipClaim::Function(FunctionName::new("fn::artifact_upload_profile_digest")?),
            OwnershipClaim::Function(FunctionName::new("fn::artifact_upload_authority_matches")?),
        ])
        .lane(MigrationLane::empty())
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new(
            "gateway",
        )?)])
        .build()
}
