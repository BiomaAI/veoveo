//! Identity persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::*;

pub const CURRENT_SCHEMA: &str = concat!(
    include_str!("identity/migrations/0000_current.surql"),
    include_str!("identity/migrations/0000_admission_apis.surql")
);

fn actor_admitted_v1_api() -> Result<KernelSqlApi, DeclarationError> {
    KernelSqlApi::new(
        FunctionName::new("fn::kernel::identity::actor_admitted_v1")?,
        MigrationVersion::new(0),
        SqlSignature::new(
            vec![
                SqlParameter::new("tenant", SqlType::Record(TableName::new("tenant")?))?,
                SqlParameter::new("context", SqlType::Record(TableName::new("work_context")?))?,
                SqlParameter::new("principal", SqlType::Record(TableName::new("principal")?))?,
                SqlParameter::new("context_digest", SqlType::String)?,
                SqlParameter::new("require_human", SqlType::Bool)?,
            ],
            SqlType::Bool,
        )?,
        SqlReadProfile::new(vec![
            TableName::new("tenant")?,
            TableName::new("work_context")?,
            TableName::new("principal")?,
        ])?,
        include_str!("identity/migrations/actor_admitted_v1.surql"),
    )
}

fn identity_enabled_v1_api() -> Result<KernelSqlApi, DeclarationError> {
    KernelSqlApi::new(
        FunctionName::new("fn::kernel::identity::identity_enabled_v1")?,
        MigrationVersion::new(0),
        SqlSignature::new(
            vec![
                SqlParameter::new("tenant", SqlType::Record(TableName::new("tenant")?))?,
                SqlParameter::new("context", SqlType::Record(TableName::new("work_context")?))?,
                SqlParameter::new("principal", SqlType::Record(TableName::new("principal")?))?,
            ],
            SqlType::Bool,
        )?,
        SqlReadProfile::new(vec![
            TableName::new("tenant")?,
            TableName::new("work_context")?,
            TableName::new("principal")?,
        ])?,
        include_str!("identity/migrations/identity_enabled_v1.surql"),
    )
}

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("identity")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Function(FunctionName::new(
                "fn::kernel::identity::identity_enabled_v1",
            )?),
            OwnershipClaim::Function(FunctionName::new(
                "fn::kernel::identity::actor_admitted_v1",
            )?),
            OwnershipClaim::Table(TableName::new("tenant")?),
            OwnershipClaim::Table(TableName::new("enterprise")?),
            OwnershipClaim::Table(TableName::new("principal")?),
            OwnershipClaim::Table(TableName::new("principal_group")?),
            OwnershipClaim::Table(TableName::new("membership")?),
            OwnershipClaim::Table(TableName::new("work_context")?),
            OwnershipClaim::Table(TableName::new("oauth_client")?),
        ])
        .lane(MigrationLane::new(vec![veoveo_modules::Migration::new(
            veoveo_modules::MigrationVersion::new(0),
            veoveo_modules::MigrationName::new("current")?,
            CURRENT_SCHEMA,
        )?])?)
        .sql_apis(vec![actor_admitted_v1_api()?, identity_enabled_v1_api()?])
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("store")?)])
        .build()
}
