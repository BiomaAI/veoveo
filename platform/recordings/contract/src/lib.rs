//! Recording domain contracts shared by producers, playback and analysis consumers.

pub mod ingest;
pub use ingest::{
    RECORDING_INGEST_SECTION, RECORDING_TARGET_GROUP, RecordingApplicationId, RecordingCatalog,
    RecordingCatalogError, RecordingCatalogSection, RecordingDatasetName, RecordingIngestResource,
    RecordingIngestStreamId, RecordingIngestUri, RecordingProducerBlueprintPolicy,
    RecordingProducerId, RecordingProducerQuotas, RecordingProducerRegistration,
    RecordingRetentionPolicy, RecordingTarget, register_catalog, target_audit_resource,
};

mod actions;
pub use actions::RecordingAction;

pub mod uris;

mod cursor;
mod ids;
mod redap;
mod resources;
mod scopes;
pub use cursor::{RECORDING_PAGE_SIZE, RecordingCatalogCursor};
pub use ids::{
    RecordingContractError, RecordingDatasetId, RecordingId, RecordingLayerId,
    RecordingProjectionId, RecordingReadGrantId,
};
pub use redap::{PlaybackArchiveUri, RecordingCatalogUri, RecordingRedapOrigin};
pub use resources::{RecordingDocument, RecordingLayersUri, RecordingResource, RecordingUri};
pub use scopes::{RecordingProducerScope, RecordingScope};

mod playback;
pub use playback::{
    PLAYBACK_MANIFEST_SCHEMA, PlaybackAccess, PlaybackArchive, PlaybackBlueprint,
    PlaybackLiveReceiver, PlaybackLiveTransport, PlaybackManifest, PlaybackManifestBuilder,
    PlaybackManifestSchema, PlaybackMapProvider, RecordingState,
};

mod catalog;
pub use catalog::{
    CreateRecordingCatalogGrantRequest, RECORDING_CATALOG_GRANT_SCHEMA, RecordingCatalogGrant,
    RecordingCatalogGrantBuilder, RecordingCatalogGrantSchema,
};
mod projection;
pub use projection::{
    CreateRecordingProjectionRequest, CreateRecordingProjectionRequestBuilder,
    MAX_PROJECTION_BYTES, MAX_PROJECTION_COMPONENTS, MAX_PROJECTION_DEADLINE_MS,
    MAX_PROJECTION_ENTITIES, MAX_PROJECTION_FRAME_REFERENCES, MAX_PROJECTION_ROWS,
    MAX_PROJECTION_SAMPLES, MAX_PROJECTION_SELECTOR_BYTES, RECORDING_PROJECTION_HANDLE_SCHEMA,
    RecordingProjectionHandle, RecordingProjectionHandleBuilder, RecordingProjectionHandleSchema,
    RecordingProjectionQuery, RecordingProjectionQueryBuilder, RecordingProjectionResultMetadata,
    RecordingProjectionSampling, RecordingProjectionSparseFill,
};

mod checked;
mod layers;
mod sealing;
mod views;
pub use layers::{
    LayerView, LayerViewBuilder, ManifestLayer, ManifestLayerBuilder, RecordingLayerKind,
    RecordingLayerState,
};
pub use sealing::{
    ManifestBlueprint, RECORDING_MANIFEST_SCHEMA, RecordingManifest, RecordingManifestBuilder,
    RecordingManifestSchema, SealRecordingOutput, SealRecordingOutputBuilder, SealRecordingRequest,
};
pub use views::{RecordingCatalogPage, RecordingView, RecordingViewBuilder};

#[cfg(feature = "policy")]
pub mod policy;
