//! Computers persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TableName, TablePrefix,
};

pub const CURRENT_SCHEMA: &str = concat!(
    include_str!("schema/migrations/0000_current.surql"),
    include_str!("schema/migrations/0000_controlled_fields.surql"),
    include_str!("schema/migrations/0000_maintenance_fields.surql"),
    include_str!("schema/migrations/0000_indexes.surql"),
);

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("computers")?, ModuleLayer::Optional)
        .ownership(vec![
            OwnershipClaim::TablePrefix(TablePrefix::new("computer_")?),
            OwnershipClaim::Table(TableName::new("computer")?),
        ])
        .lane(MigrationLane::new(vec![veoveo_modules::Migration::new(
            veoveo_modules::MigrationVersion::new(0),
            veoveo_modules::MigrationName::new("current")?,
            CURRENT_SCHEMA,
        )?])?)
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("audit")?)])
        .build()
}

/// Owner-declared native LIVE and recoverable changefeed sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, veoveo_types::Vocabulary)]
pub enum ComputerObservationTable {
    #[vocabulary(rename = "computer")]
    Computer,
    #[vocabulary(rename = "computer_automation_policy")]
    AutomationPolicy,
    #[vocabulary(rename = "computer_session_grant_policy")]
    SessionGrantPolicy,
    #[vocabulary(rename = "computer_execution")]
    Execution,
    #[vocabulary(rename = "computer_file_transfer")]
    FileTransfer,
    #[vocabulary(rename = "computer_automation_grant")]
    AutomationGrant,
    #[vocabulary(rename = "computer_session_grant")]
    SessionGrant,
    #[vocabulary(rename = "computer_cli_grant")]
    CliGrant,
    #[vocabulary(rename = "computer_maintenance")]
    Maintenance,
}

impl From<ComputerObservationTable> for veoveo_modules::ObservationTable {
    fn from(table: ComputerObservationTable) -> Self {
        Self::new(
            TableName::new(table.as_str()).expect("checked owner table declaration"),
            veoveo_modules::ObservationReplay::Changefeed(
                veoveo_modules::ChangefeedRetention::from_days(30)
                    .expect("qualified owner retention"),
            ),
        )
    }
}
