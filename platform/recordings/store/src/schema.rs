//! Recordings persistence declaration; the composition root supplies its lane execution host.

use veoveo_modules::{
    DeclarationError, LaneExecution, LaneRequirement, MigrationLane, ModuleLayer, ModuleName,
    ModuleSetup, OwnershipClaim, TableName, TablePrefix,
};

pub const MANIFEST_PUBLICATION_SCHEMA: &str =
    include_str!("schema/migrations/0001_manifest_publication.surql");

pub const CURRENT_SCHEMA: &str = include_str!("schema/migrations/0000_current.surql");

/// Declare target ownership and dependencies without applying the mixed Store catalog.
pub fn module_setup(execution: LaneExecution) -> Result<ModuleSetup, DeclarationError> {
    ModuleSetup::builder(ModuleName::new("recordings")?, ModuleLayer::Optional)
        .ownership(vec![
            OwnershipClaim::TablePrefix(TablePrefix::new("recording_")?),
            OwnershipClaim::Table(TableName::new("recording")?),
        ])
        .lane(MigrationLane::new(vec![
            veoveo_modules::Migration::new(
                veoveo_modules::MigrationVersion::new(0),
                veoveo_modules::MigrationName::new("current")?,
                CURRENT_SCHEMA,
            )?,
            veoveo_modules::Migration::new(
                veoveo_modules::MigrationVersion::new(1),
                veoveo_modules::MigrationName::new("manifest_publication")?,
                MANIFEST_PUBLICATION_SCHEMA,
            )?,
        ])?)
        .execution(execution)
        .requires(vec![LaneRequirement::Satisfied(ModuleName::new("tasks")?)])
        .build()
}

/// Owner-declared native LIVE and recoverable changefeed sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, veoveo_types::Vocabulary)]
pub enum RecordingObservationTable {
    #[vocabulary(rename = "recording_dataset")]
    Dataset,
    #[vocabulary(rename = "recording")]
    Recording,
    #[vocabulary(rename = "recording_layer")]
    Layer,
    #[vocabulary(rename = "recording_read_grant")]
    ReadGrant,
    #[vocabulary(rename = "recording_projection_receipt")]
    ProjectionReceipt,
    #[vocabulary(rename = "recording_ingest_stream")]
    IngestStream,
    #[vocabulary(rename = "recording_ingest_batch")]
    IngestBatch,
    #[vocabulary(rename = "recording_blueprint")]
    Blueprint,
}

impl From<RecordingObservationTable> for veoveo_modules::ObservationTable {
    fn from(table: RecordingObservationTable) -> Self {
        Self::new(
            TableName::new(table.as_str()).expect("checked owner table declaration"),
            veoveo_modules::ObservationReplay::Changefeed(
                veoveo_modules::ChangefeedRetention::from_days(30)
                    .expect("qualified owner retention"),
            ),
        )
    }
}
