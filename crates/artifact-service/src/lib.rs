//! The shared artifact plane: a single byte-level policy-enforcement point.
//!
//! Domain servers (media, timeseries, optimization, duckdb) stop owning private
//! buckets and call this service with the gateway-signed identity they already
//! hold. It owns the object store and SurrealDB artifact state, and decides
//! every read/write with the contract's
//! [`veoveo_mcp_contract::access::decide`].

pub mod auth;
pub mod config;
pub mod http;
pub mod ledger;
pub mod service;
pub mod store;

pub use auth::PlaneAuthenticator;
pub use config::{Config, ObjectStoreConfig};
pub use ledger::surreal::SurrealArtifactRepository;
pub use ledger::{ArtifactRepository, StoredArtifact};
pub use service::{ArtifactDownload, ArtifactService, DownloadDelivery};
pub use store::{ArtifactObjectStore, BlobStore};
