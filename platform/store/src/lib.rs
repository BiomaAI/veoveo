//! Authoritative SurrealDB-backed platform state for Veoveo installations.
//!
//! Domain services own their behavior. This crate owns the shared typed records,
//! schema migrations, durable outbox, changefeed replay, and LIVE subscriptions
//! used to coordinate those services.

mod administration;
pub mod agent_management;
mod artifact_access_requests;
mod artifact_reads;
mod artifact_uploads;
mod artifacts;
mod changefeed;
mod config;
mod error;
mod gateway_control;
mod gateway_retention;
mod gateway_runtime;
mod governance;
mod identity;
mod ids;
mod live_views;
mod map;
mod map_authoring;
mod map_derivations;
mod map_presentations;
mod map_projection;
mod migration_preparation;
mod migrations;
mod models;
mod outbox;
mod recording_blueprints;
mod recording_catalog;
mod recording_ingest;
mod recordings;
mod resource_changes;
mod store;
mod table;
mod task_ids;
mod usage;
pub mod workspace;

pub use artifact_access_requests::{
    ArtifactAccessRequestDecisionDraft, ArtifactAccessRequestDraft, ArtifactAccessRequestQuery,
};
pub use artifact_reads::{
    ArtifactReadAdmission, ArtifactReadCapabilityDraft, ArtifactReadCapabilityRecord,
    ArtifactReadContextVersion, ArtifactReadMembership,
};
pub use artifact_uploads::*;
pub use artifacts::{
    ArtifactAggregate, ArtifactAuditDraft, ArtifactGrantDraft, ArtifactOccurrenceDraft,
    ArtifactShareLinkDraft, ArtifactWriteCapabilityDraft, ArtifactWriteReservation,
    PublicShareRedemption,
};
pub use changefeed::{
    ChangefeedBatch, ChangefeedCursor, ChangefeedEntry, LiveStream, decode_changefeed_entry,
};
pub use config::{StoreAuthLevel, StoreConfig, StoreConfigBuilder, StoreCredentials};
pub use error::{MigrationError, RecordingIngestQuota, StoreConfigError, StoreError};
pub use gateway_retention::GATEWAY_AUDIT_BATCH_LIMIT;
pub use gateway_runtime::{
    GatewayAuditKind, GatewayRefreshRedelivery, GatewayRefreshRetentionSummary,
    GatewayRefreshRotation, GatewayRefreshRotationOutcome, gateway_authorization_code_record_id,
    gateway_authorization_request_record_id, gateway_jwt_revocation_record_id,
    gateway_refresh_family_record_id, gateway_refresh_token_record_id, gateway_replay_record_id,
    gateway_resource_subscription_record_id,
};
pub use identity::{
    PlatformIdentity, deterministic_enterprise_id, deterministic_group_id,
    deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};
pub use ids::*;
pub use map::{
    MapAcquisitionDraft, MapAcquisitionUpdate, MapCatalogCompletion, MapMatrixIndexRecord,
    MapMobilityProfileDraft, MapOperationalSnapshotDraft, MapReleaseDraft, MapRestrictionDraft,
    MapRouteDependencyDraft, MapRouteDraft, MapRouteIndexRecord, MapRouteMatrixDraft,
    MapSourceDraft,
};
pub use map_authoring::{
    MapAuthoringCompletion, MapAuthoringReadScope, MapFeatureCommitDraft, MapFeatureCommitResult,
    MapFeatureLayerDraft, MapFeatureLayerUpdateDraft, MapFeatureRevisionDraft,
    MapFeatureSchemaDraft, MapLayerPublicationDraft, MapStyleRevisionDraft,
    map_authoring_idempotency_key,
};
pub use map_derivations::{
    MapDerivationDraft, MapDerivationKind, MapDerivationRecord, MapDerivationScope,
    MapDerivationSummary,
};
pub use map_presentations::{
    MapCompositionDraft, MapCompositionRevisionDraft, MapCompositionUpdateDraft,
    MapLayerProductDraft,
};
pub use map_projection::MapFeatureProjectionCommit;
pub use migrations::{
    AppliedMigration, DownstreamMigration, DownstreamMigrationError, DownstreamSchemaStatus,
    Migration, MigrationReport, SchemaStatus, migrations, schema_sql, validate_catalog,
};
pub use models::*;
pub use outbox::{OutboxDraft, OutboxPage};
pub use recording_blueprints::{
    RecordingBlueprintCommit, RecordingBlueprintDraft, RecordingBlueprintOutcome,
};
pub use recording_catalog::{
    RecordingCatalogCleanup, RecordingDatasetDraft, RecordingLayerDraft,
    RecordingProjectionReceiptDraft, RecordingReadGrantDraft, capture_layer_name,
};
pub use recording_ingest::{
    RecordingIngestAppendOutcome, RecordingIngestBatchDraft, RecordingIngestQuotaCheckpoint,
    RecordingIngestStreamDraft,
};
pub use recordings::{
    RecordingCursor, RecordingDraft, RecordingLayerCounts, RecordingReadScope, RecordingSeal,
};
pub use resource_changes::{ResourceChangeTable, ResourceInvalidation};
pub use store::{PlatformClient, PlatformStore, primary_transaction_error};
pub use surrealdb::types::{RecordId, RecordIdKey, Value};
pub use table::PlatformTable;
pub use task_ids::task_record_id;
pub use usage::{DomainUsageDraft, DomainUsageTaskPage};
