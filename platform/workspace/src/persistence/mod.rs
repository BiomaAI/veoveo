//! Owner persistence over the shared installation Store connection.
mod agent_import;
pub use agent_import::*;
mod ids;
pub use ids::*;
mod repository;
pub use repository::*;

use veoveo_platform_store::{PlatformClient, PlatformStore};

#[derive(Clone, Debug)]
pub struct WorkspaceRepository {
    store: PlatformStore,
}
impl WorkspaceRepository {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }
    pub fn platform(&self) -> &PlatformStore {
        &self.store
    }
    fn client(&self) -> &surrealdb::Surreal<PlatformClient> {
        self.store.client()
    }
}
