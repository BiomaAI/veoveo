//! Governed analysis of Rerun video recordings using an explicit NVIDIA
//! DeepStream/TensorRT execution boundary.

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
pub mod uris;

#[cfg(feature = "schema")]
pub mod schema;
#[cfg(feature = "mcp")]
pub mod task_lookup;

#[cfg(feature = "mcp")]
pub mod task_request;

#[cfg(feature = "mcp")]
pub mod task_product;
