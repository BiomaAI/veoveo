//! Governed semantic and temporal reasoning over Rerun video recordings using
//! an explicit local world-model execution boundary.

#[cfg(feature = "runtime")]
pub mod annotation;
#[cfg(feature = "runtime")]
pub mod artifacts;
#[cfg(feature = "runtime")]
pub mod catalog;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod executor;
#[cfg(feature = "runtime")]
pub mod grounding;
#[cfg(feature = "contract")]
pub mod uris;
