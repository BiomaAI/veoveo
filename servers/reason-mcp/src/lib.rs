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
#[cfg(feature = "contract")]
pub mod grounding;
#[cfg(feature = "mcp")]
pub mod knowledge;
#[cfg(feature = "contract")]
pub mod uris;

#[cfg(feature = "schema")]
pub mod schema;
#[cfg(feature = "mcp")]
pub mod task_lookup;

#[cfg(feature = "mcp")]
pub mod task_request;

#[cfg(feature = "mcp")]
pub mod task_product;
