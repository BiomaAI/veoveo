//! Authoritative temporal reasoning for Veoveo agents.

#[cfg(feature = "runtime")]
pub mod authority;
#[cfg(feature = "runtime")]
pub mod catalog;
#[cfg(feature = "runtime")]
pub mod clock;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod engine;
#[cfg(feature = "runtime")]
pub mod registry;

#[cfg(feature = "contract")]
pub use contract::*;

#[cfg(feature = "runtime")]
pub mod acquisition;
#[cfg(feature = "mcp")]
mod admin;
#[cfg(feature = "runtime")]
mod index;
#[cfg(feature = "mcp")]
pub mod mcp;
#[cfg(feature = "mcp")]
pub mod prompts;
#[cfg(feature = "mcp")]
mod server;
#[cfg(feature = "runtime")]
pub mod state;
#[cfg(feature = "contract")]
pub mod uris;

#[cfg(feature = "mcp")]
pub async fn run() -> anyhow::Result<()> {
    server::run().await
}

#[cfg(feature = "runtime")]
mod persistence;
#[cfg(all(test, feature = "runtime"))]
#[path = "../../../testing/fixtures/store.rs"]
mod test_store;

#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "schema")]
pub mod observation;
#[cfg(feature = "schema")]
pub use observation::TimeObservationTable;

#[cfg(all(test, feature = "runtime"))]
async fn test_database(backend: test_store::StoreBackend) -> test_store::TestDb {
    test_store::TestDb::with_backend_and_modules(
        backend,
        vec![schema::module_setup(test_store::module_lanes::execution("time").unwrap()).unwrap()],
    )
    .await
}
