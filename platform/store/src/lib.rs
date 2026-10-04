//! Authoritative SurrealDB-backed platform state for Veoveo installations.
//!
//! Domain services own their behavior. This crate owns the shared typed records,
//! schema migrations, changefeed replay and LIVE subscriptions
//! used to coordinate those services.

#[cfg(feature = "runtime")]
mod administration;
#[cfg(feature = "runtime")]
pub mod agent_management;
#[cfg(feature = "runtime")]
mod artifact_access_requests;
#[cfg(feature = "runtime")]
mod artifact_reads;
#[cfg(feature = "runtime")]
mod artifact_uploads;
#[cfg(feature = "runtime")]
mod artifacts;
#[cfg(feature = "runtime")]
pub mod audit;
#[cfg(feature = "runtime")]
mod changefeed;
#[cfg(feature = "runtime")]
mod config;
#[cfg(feature = "runtime")]
mod error;
#[cfg(feature = "runtime")]
mod gateway_control;
#[cfg(feature = "runtime")]
mod gateway_runtime;
#[cfg(feature = "runtime")]
mod governance;
#[cfg(feature = "runtime")]
mod identity;
#[cfg(feature = "runtime")]
mod ids;
#[cfg(feature = "runtime")]
mod json_value;
#[cfg(feature = "runtime")]
pub mod knowledge;
#[cfg(feature = "runtime")]
mod map;
#[cfg(feature = "runtime")]
mod map_authoring;
#[cfg(feature = "runtime")]
mod map_derivations;
#[cfg(feature = "runtime")]
mod map_presentations;
#[cfg(feature = "runtime")]
mod map_projection;
#[cfg(feature = "runtime")]
mod migrations;
#[cfg(feature = "runtime")]
mod models;
#[cfg(feature = "runtime")]
mod recording_blueprints;
#[cfg(feature = "runtime")]
mod recording_catalog;
#[cfg(feature = "runtime")]
mod recording_ingest;
#[cfg(feature = "runtime")]
mod recordings;
#[cfg(feature = "runtime")]
mod resource_changes;
#[cfg(feature = "runtime")]
mod store;
#[cfg(feature = "runtime")]
mod table;
#[cfg(feature = "runtime")]
mod task_ids;
#[cfg(feature = "runtime")]
mod task_result;
#[cfg(feature = "runtime")]
mod usage;
#[cfg(feature = "runtime")]
pub mod workspace;

#[cfg(feature = "runtime")]
pub use artifact_access_requests::{
    ArtifactAccessRequestDecisionDraft, ArtifactAccessRequestDraft, ArtifactAccessRequestQuery,
};
#[cfg(feature = "runtime")]
pub use artifact_reads::{
    ArtifactReadAdmission, ArtifactReadCapabilityDraft, ArtifactReadCapabilityRecord,
    ArtifactReadContextVersion, ArtifactReadMembership,
};
#[cfg(feature = "runtime")]
pub use artifact_uploads::*;
#[cfg(feature = "runtime")]
pub use artifacts::{
    ArtifactAggregate, ArtifactGrantDraft, ArtifactOccurrenceDraft, ArtifactReadScope,
    ArtifactShareLinkDraft, ArtifactWriteCapabilityDraft, ArtifactWriteReservation,
    PublicShareRedemption,
};
#[cfg(feature = "runtime")]
pub use changefeed::{
    ArtifactChange, ChangefeedBatch, ChangefeedConsumerId, ChangefeedCursor, ChangefeedDelivery,
    ChangefeedEntry, ComputerChange, LiveStream, TaskChange, decode_changefeed_entry,
};
#[cfg(feature = "runtime")]
pub use config::{StoreAuthLevel, StoreConfig, StoreConfigBuilder, StoreCredentials};
#[cfg(feature = "runtime")]
pub use error::{MigrationError, RecordingIngestQuota, StoreConfigError, StoreError};
#[cfg(feature = "runtime")]
pub use gateway_runtime::{
    GatewayRefreshRedelivery, GatewayRefreshRetentionSummary, GatewayRefreshRotation,
    GatewayRefreshRotationOutcome, gateway_authorization_code_record_id,
    gateway_authorization_request_record_id, gateway_jwt_revocation_record_id,
    gateway_refresh_family_record_id, gateway_refresh_token_record_id, gateway_replay_record_id,
    gateway_resource_subscription_record_id,
};
#[cfg(feature = "runtime")]
pub use identity::{
    PlatformIdentity, PrincipalIdentityRef, deterministic_enterprise_id, deterministic_group_id,
    deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};
#[cfg(feature = "runtime")]
pub use ids::*;
#[cfg(feature = "runtime")]
pub use map::{
    MapAcquisitionDraft, MapAcquisitionUpdate, MapCatalogCompletion, MapMatrixIndexRecord,
    MapMobilityProfileDraft, MapOperationalSnapshotDraft, MapReleaseDraft, MapRestrictionDraft,
    MapRouteDependencyDraft, MapRouteDraft, MapRouteIndexRecord, MapRouteMatrixDraft,
    MapSourceDraft,
};
#[cfg(feature = "runtime")]
pub use map_authoring::{
    MapAuthoringCompletion, MapAuthoringReadScope, MapFeatureCommitDraft, MapFeatureCommitResult,
    MapFeatureLayerDraft, MapFeatureLayerUpdateDraft, MapFeatureRevisionDraft,
    MapFeatureSchemaDraft, MapLayerPublicationDraft, MapStyleRevisionDraft,
    map_authoring_idempotency_key,
};
#[cfg(feature = "runtime")]
pub use map_derivations::{
    MapDerivationDraft, MapDerivationKind, MapDerivationRecord, MapDerivationScope,
    MapDerivationSummary,
};
#[cfg(feature = "runtime")]
pub use map_presentations::{
    MapCompositionDraft, MapCompositionRevisionDraft, MapCompositionUpdateDraft,
    MapLayerProductDraft,
};
#[cfg(feature = "runtime")]
pub use map_projection::MapFeatureProjectionCommit;
#[cfg(feature = "runtime")]
pub use migrations::{
    AppliedMigration, DownstreamMigration, DownstreamMigrationError, DownstreamSchemaStatus,
    Migration, MigrationReport, SchemaStatus, migrations, schema_sql, validate_catalog,
};
#[cfg(feature = "runtime")]
pub use models::*;
#[cfg(feature = "runtime")]
pub use recording_blueprints::{
    RecordingBlueprintCommit, RecordingBlueprintDraft, RecordingBlueprintOutcome,
};
#[cfg(feature = "runtime")]
pub use recording_catalog::{
    RecordingAccessScope, RecordingCatalogCleanup, RecordingDatasetDraft, RecordingLayerDraft,
    RecordingProjectionReceiptDraft, RecordingProjectionRequest, RecordingReadGrantDraft,
    RecordingReadGrantRequest, capture_layer_name,
};
#[cfg(feature = "runtime")]
pub use recording_ingest::{
    RecordingIngestAppendOutcome, RecordingIngestBatchDraft, RecordingIngestQuotaCheckpoint,
    RecordingIngestStreamDraft,
};
#[cfg(feature = "runtime")]
pub use recordings::{
    RecordingCursor, RecordingDraft, RecordingLayerCounts, RecordingReadScope, RecordingSeal,
};
#[cfg(feature = "runtime")]
pub use resource_changes::{ResourceChangeTable, ResourceInvalidation};
#[cfg(feature = "runtime")]
pub use store::{PlatformClient, PlatformStore, primary_transaction_error};
#[cfg(feature = "runtime")]
pub use surrealdb::types::{RecordId, RecordIdKey, Value};
#[cfg(feature = "runtime")]
pub use table::PlatformTable;
#[cfg(feature = "runtime")]
pub use task_ids::task_record_id;
#[cfg(feature = "runtime")]
pub use task_result::TaskResultRecord;
#[cfg(feature = "runtime")]
pub use usage::DomainUsageDraft;

#[cfg(feature = "schema")]
pub mod schema;
