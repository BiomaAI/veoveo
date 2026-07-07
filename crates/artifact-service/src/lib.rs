//! The shared artifact plane: a single byte-level policy-enforcement point.
//!
//! Domain servers (media, timeseries, optimization, duckdb) stop owning private
//! buckets and call this service with the gateway-signed identity they already
//! hold. It owns the object store (per-tenant encrypted) and the Postgres grant
//! ledger, and decides every read/write with the contract's
//! [`veoveo_mcp_contract::access::decide`].

pub mod auth;
pub mod config;
pub mod crypto;
pub mod http;
pub mod ledger;
pub mod service;
pub mod store;

pub use auth::PlaneAuthenticator;
pub use config::{Config, ObjectStoreConfig};
pub use crypto::{MasterKey, TenantCipher};
pub use ledger::postgres::PostgresGrantLedger;
pub use ledger::{GrantLedger, StoredArtifact};
pub use service::ArtifactService;
pub use store::{BlobStore, EncryptedObjectStore};
