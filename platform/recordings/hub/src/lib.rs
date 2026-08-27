//! The Recording Hub: a durable, queryable time-and-space record.
//!
//! Producers (sensors, estimators, agents' tees) stream Rerun data into the
//! spooler's embedded gRPC proxy; the spooler persists every message as
//! day-partitioned segment files (`{dataset}/{day}/{recording}.rrd`); the OSS
//! Rerun catalog server (`rerun server`) serves the same files as queryable
//! datasets. Files are the record, the proxy is the bus, the catalog is the
//! reading room.

pub mod archive;
pub mod blueprint;
pub mod catalog;
pub mod config;
mod diagnostics;
mod governance;
pub mod ingest;
pub mod ingest_http;
pub mod layer_files;
pub mod live;
pub mod publication;
pub mod sim;
pub mod spool;
pub mod video;

pub use archive::{ArchiveMaterialization, materialize_archive_shard};
pub use blueprint::{BlueprintMapProviderSelection, ValidatedBlueprint, validate_blueprint_rrd};
pub use catalog::{CatalogPolicy, PlatformCatalog, SegmentInspection, inspect_segment};
pub use config::{DatasetName, DatasetRoute, QUARANTINE_DATASET, SpoolerConfig};
pub use diagnostics::{
    RECORDING_INGEST_DIAGNOSTICS_SCHEMA, RecordingIngestDiagnostics,
    RecordingIngestDiagnosticsDocument,
};
pub use governance::invocation_authority_record;
pub use ingest::{
    RecordingBlueprintPublicationError, RecordingIngestService, RecordingIngestServiceConfig,
    ingest_part_paths, ingest_part_sequence, ingest_recording_static_context_path,
    ingest_segment_parts_directory, live_segment_byte_len,
};
pub use ingest_http::recording_ingest_internal_router;
pub use layer_files::{RecordingLayerFileScope, collect_recording_layer_files};
pub use live::{LiveRrdBatchKind, optimize_live_rrd_messages};
pub use publication::{GatewayLayerPublisher, GatewayLayerPublisherConfig};
pub use sim::{
    Generator, LatLon, Sample, SensorId, SensorKind, SensorReport, SensorSpec, SensorStack,
    StackReport, TrackPattern, Wave,
};
pub use spool::{
    Counters, FrozenSegment, OpenedSegment, PublishedBlueprint, SegmentCatalog, SegmentKey,
    Spooler, run_blocking,
};
pub use veoveo_recording_forwarder::config::ClientAssertionAlgorithm;
pub use veoveo_rrd::video::{
    RrdVideoBoundary, h264_access_unit_is_decoder_reentrant, inspect_log_message_video_boundary,
    inspect_rrd_video_boundary,
};
pub use video::{
    EncodedVideoClip, EncodedVideoSample, H264VideoProfile, VideoClipRequest, VideoIndexKind,
    extract_video_clip, extract_video_clip_from_messages, remux_h264_mp4,
};
