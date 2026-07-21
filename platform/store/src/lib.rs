//! Authoritative SurrealDB-backed platform state for Veoveo installations.
//!
//! Domain services own their behavior. This crate owns the shared typed records,
//! schema migrations, durable outbox, changefeed replay, and LIVE subscriptions
//! used to coordinate those services.

mod administration;
mod artifact_access_requests;
mod artifacts;
mod changefeed;
mod config;
mod coordinates;
mod error;
mod gateway_runtime;
mod governance;
mod identity;
mod ids;
mod map;
mod migrations;
mod models;
mod outbox;
mod recording_ingest;
mod recordings;
mod store;
mod table;
mod time;
mod usage;

pub use artifact_access_requests::{
    ArtifactAccessRequestDecisionDraft, ArtifactAccessRequestDraft, ArtifactAccessRequestQuery,
};
pub use artifacts::{
    ArtifactAggregate, ArtifactAuditDraft, ArtifactGrantDraft, ArtifactOccurrenceDraft,
    ArtifactShareLinkDraft, ArtifactWriteCapabilityDraft, ArtifactWriteReservation,
    PublicShareRedemption,
};
pub use changefeed::{
    ChangefeedBatch, ChangefeedCursor, ChangefeedEntry, LiveStream, decode_changefeed_entry,
};
pub use config::{StoreAuthLevel, StoreConfig, StoreConfigBuilder, StoreCredentials};
pub use coordinates::{CoordinateFrameDraft, CoordinateOperationDraft};
pub use error::{MigrationError, RecordingIngestQuota, StoreConfigError, StoreError};
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
    MapAcquisitionDraft, MapMobilityProfileDraft, MapOperationalSnapshotDraft, MapReleaseDraft,
    MapRestrictionDraft, MapRouteDependencyDraft, MapRouteDraft, MapRouteMatrixDraft,
    MapSourceDraft,
};
pub use migrations::{
    AppliedMigration, Migration, MigrationReport, SchemaStatus, migrations, schema_sql,
    validate_catalog,
};
pub use models::*;
pub use outbox::{OutboxDraft, OutboxPage};
pub use recording_ingest::{
    RecordingIngestAppendOutcome, RecordingIngestBatchDraft, RecordingIngestStreamDraft,
};
pub use recordings::{RecordingDraft, RecordingSeal, SegmentDraft, SegmentSealBinding};
pub use store::{PlatformClient, PlatformStore};
pub use surrealdb::types::{RecordId, RecordIdKey, Value};
pub use table::PlatformTable;
pub use time::{
    TimeAcquisitionDraft, TimeAuthorityReleaseDraft, TimeCalendarVersionDraft,
    TimeClockPolicyDraft, TimeMissionEpochDraft, TimeSourceDraft, TimeTemporalEventDraft,
};
pub use usage::DomainUsageDraft;
