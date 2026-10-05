//! Owner persistence over the shared installation Store connection.
mod authority;
mod ids;
pub use ids::*;
mod models;
pub use models::*;
mod repository;
pub use repository::*;

use veoveo_platform_store::{PlatformClient, PlatformStore};

#[derive(Clone, Debug)]
pub struct AgentRepository {
    store: PlatformStore,
}
impl AgentRepository {
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
