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
#[cfg(feature = "runtime")]
pub mod uris;
