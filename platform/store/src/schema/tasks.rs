//! Tasks persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::*;

pub const SELECTION_V1: &str = include_str!("tasks/migrations/0000_selection_v1.surql");

fn selection_api() -> Result<KernelSqlApi, DeclarationError> {
    fn record(table: &str) -> Result<SqlType, DeclarationError> {
        Ok(SqlType::Record(TableName::new(table)?))
    }
    fn optional(kind: SqlType) -> SqlType {
        SqlType::Option(Box::new(kind))
    }
    fn array(kind: SqlType) -> SqlType {
        SqlType::Array(Box::new(kind))
    }
    let parameters = vec![
        SqlParameter::new("task", record("task")?)?,
        SqlParameter::new("server", record("mcp_server")?)?,
        SqlParameter::new("tenant", record("tenant")?)?,
        SqlParameter::new("owner", record("principal")?)?,
        SqlParameter::new("profile", record("profile")?)?,
        SqlParameter::new("principal_key", SqlType::String)?,
        SqlParameter::new("profile_key", SqlType::String)?,
        SqlParameter::new("tenant_key", optional(SqlType::String))?,
        SqlParameter::new("labels", array(SqlType::String))?,
        SqlParameter::new("task_types", optional(array(SqlType::String)))?,
        SqlParameter::new("work_context", optional(record("work_context")?))?,
        SqlParameter::new("work_context_key", optional(SqlType::String))?,
        SqlParameter::new("authority_tenant", optional(SqlType::String))?,
    ];
    KernelSqlApi::new(
        FunctionName::new("fn::kernel::tasks::selection_v1")?,
        MigrationVersion::new(0),
        SqlSignature::new(parameters, optional(SqlType::Object))?,
        SqlReadProfile::new(vec![TableName::new("task")?])?,
        SELECTION_V1,
    )
}

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("tasks")?, ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Table(TableName::new("task")?),
            OwnershipClaim::Function(FunctionName::new("fn::kernel::tasks::selection_v1")?),
            OwnershipClaim::Table(TableName::new("task_input")?),
            OwnershipClaim::Table(TableName::new("task_idempotency")?),
            OwnershipClaim::Table(TableName::new("task_produced_artifact")?),
            OwnershipClaim::Table(TableName::new("task_used_artifact")?),
            OwnershipClaim::Table(TableName::new("provider_job")?),
            OwnershipClaim::Table(TableName::new("provider_event")?),
            OwnershipClaim::Table(TableName::new("domain_usage")?),
        ])
        .lane(MigrationLane::new(vec![Migration::new(
            MigrationVersion::new(0),
            MigrationName::new("selection_v1")?,
            SELECTION_V1,
        )?])?)
        .sql_apis(vec![selection_api()?])
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new(
            "artifacts",
        )?)])
        .build()
}
