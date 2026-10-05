//! Artifacts persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::*;

pub const CURRENT_SCHEMA: &str = concat!(
    include_str!("artifacts/migrations/0000_current.surql"),
    include_str!("artifacts/migrations/read_v1.surql")
);

fn read_api() -> Result<KernelSqlApi, DeclarationError> {
    let record = |name| TableName::new(name).map(SqlType::Record);
    let array = |kind| SqlType::Array(Box::new(kind));
    let optional = |kind| SqlType::Option(Box::new(kind));
    KernelSqlApi::new(
        FunctionName::new("fn::kernel::artifacts::read_v1")?,
        MigrationVersion::new(0),
        SqlSignature::new(
            vec![
                SqlParameter::new("artifact", record("artifact_occurrence")?)?,
                SqlParameter::new("tenant", record("tenant")?)?,
                SqlParameter::new("principals", array(record("principal")?))?,
                SqlParameter::new("groups", array(record("principal_group")?))?,
                SqlParameter::new("clearance", array(SqlType::String))?,
                SqlParameter::new("context", optional(record("work_context")?))?,
                SqlParameter::new("context_key", optional(SqlType::String))?,
                SqlParameter::new("at", SqlType::Datetime)?,
            ],
            optional(SqlType::Object),
        )?,
        SqlReadProfile::new(vec![
            TableName::new("artifact_occurrence")?,
            TableName::new("artifact_grant")?,
        ])?,
        include_str!("artifacts/migrations/read_v1.surql"),
    )
}

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("artifacts")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Function(FunctionName::new("fn::kernel::artifacts::read_v1")?),
            OwnershipClaim::TablePrefix(TablePrefix::new("artifact_")?),
            OwnershipClaim::Table(TableName::new("share_link")?),
            OwnershipClaim::Function(FunctionName::new("fn::artifact_upload_profile_digest")?),
            OwnershipClaim::Function(FunctionName::new("fn::artifact_upload_authority_matches")?),
        ])
        .lane(MigrationLane::new(vec![veoveo_modules::Migration::new(
            veoveo_modules::MigrationVersion::new(0),
            veoveo_modules::MigrationName::new("current")?,
            CURRENT_SCHEMA,
        )?])?)
        .sql_apis(vec![read_api()?])
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new(
            "gateway",
        )?)])
        .build()
}
