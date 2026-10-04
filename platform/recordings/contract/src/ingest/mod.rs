mod ids;
pub use ids::{
    RecordingApplicationId, RecordingDatasetName, RecordingIngestStreamId, RecordingProducerId,
};

mod config;
pub use config::*;

mod target;
pub use target::RecordingTarget;

mod catalog;
pub use catalog::{RecordingCatalog, RecordingCatalogError};

mod registration;
pub use registration::{
    RECORDING_INGEST_SECTION, RECORDING_TARGET_GROUP, RecordingCatalogSection, register_catalog,
};

mod audit;
pub use audit::{RecordingIngestUri, target_audit_resource};
