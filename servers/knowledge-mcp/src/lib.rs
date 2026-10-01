//! Knowledge discovery, indexing and retrieval over approved source resources.
#[cfg(feature = "runtime")]
pub mod access;
#[cfg(feature = "runtime")]
pub mod chunk;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod embed;
#[cfg(feature = "runtime")]
mod error;
#[cfg(feature = "runtime")]
pub mod index;
#[cfg(feature = "runtime")]
pub mod search;
#[cfg(feature = "runtime")]
pub mod source;
#[cfg(feature = "runtime")]
pub use error::ServiceError;
