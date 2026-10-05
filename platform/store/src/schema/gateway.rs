//! Gateway persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::*;

pub const CURRENT_SCHEMA: &str = concat!(
    include_str!("gateway/migrations/0000_current.surql"),
    include_str!("gateway/migrations/0000_admission_apis.surql")
);

fn task_retention_route_v1_api() -> Result<KernelSqlApi, DeclarationError> {
    KernelSqlApi::new(
        FunctionName::new("fn::kernel::gateway::task_retention_route_v1")?,
        MigrationVersion::new(0),
        SqlSignature::new(
            vec![SqlParameter::new(
                "route",
                SqlType::Record(TableName::new("gateway_task_route")?),
            )?],
            SqlType::Object,
        )?,
        SqlReadProfile::new(vec![TableName::new("gateway_task_route")?])?,
        include_str!("gateway/migrations/0000_admission_apis.surql"),
    )
}

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("gateway")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Function(FunctionName::new(
                "fn::kernel::gateway::task_retention_route_v1",
            )?),
            OwnershipClaim::TablePrefix(TablePrefix::new("gateway_")?),
            OwnershipClaim::Table(TableName::new("policy_revision")?),
            OwnershipClaim::Table(TableName::new("profile")?),
            OwnershipClaim::Table(TableName::new("profile_server")?),
            OwnershipClaim::Table(TableName::new("mcp_server")?),
            OwnershipClaim::Table(TableName::new("mcp_interaction")?),
        ])
        .lane(MigrationLane::new(vec![veoveo_modules::Migration::new(
            veoveo_modules::MigrationVersion::new(0),
            veoveo_modules::MigrationName::new("current")?,
            CURRENT_SCHEMA,
        )?])?)
        .sql_apis(vec![task_retention_route_v1_api()?])
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new(
            "identity",
        )?)])
        .build()
}
