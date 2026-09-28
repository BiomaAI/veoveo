//! Governed recorded-video access shared by every server that consumes Rerun
//! `VideoStream` recordings.
//!
//! One selection contract, one bounded materialization path: authorize the
//! canonical recording identity, capture the immutable archive segments and
//! acknowledged live ingest parts visible at task start, extract the requested
//! range from the preceding decoder-reentrant keyframe, and remux to MP4
//! without re-encoding. Servers own what happens to the materialized clip;
//! this crate owns how recorded video is reached.

#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod runtime;
