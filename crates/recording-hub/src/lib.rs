//! The Recording Hub: a durable, queryable time-and-space record.
//!
//! Producers (sensors, estimators, agents' tees) stream Rerun data into the
//! spooler's embedded gRPC proxy; the spooler persists every message as
//! day-partitioned segment files (`{dataset}/{day}/{recording}.rrd`); the OSS
//! Rerun catalog server (`rerun server`) serves the same files as queryable
//! datasets. Files are the record, the proxy is the bus, the catalog is the
//! reading room.

pub mod config;
pub mod query;
pub mod sim;
pub mod spool;

pub use config::{DatasetName, DatasetRoute, QUARANTINE_DATASET, SpoolerConfig};
pub use query::{QueryResult, collect_segments, query_tree};
pub use sim::{
    FleetReport, Generator, LatLon, Sample, SensorFleet, SensorId, SensorKind, SensorReport,
    SensorSpec, TrackPattern, Wave,
};
pub use spool::{Counters, SegmentKey, Spooler, run_blocking};
