//! Owner persistence over the shared installation Store connection.
#[cfg(feature = "persistence")]
mod persistence;
#[cfg(feature = "schema")]
pub mod schema;
#[cfg(feature = "persistence")]
pub use persistence::*;
