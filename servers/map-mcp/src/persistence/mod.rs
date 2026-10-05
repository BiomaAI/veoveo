//! Owner persistence over the shared installation Store connection.
mod models;
pub use models::*;
mod map;
pub use map::*;
mod map_authoring;
pub use map_authoring::*;
mod map_derivations;
pub use map_derivations::*;
mod map_presentations;
pub use map_presentations::*;
mod map_projection;
pub use map_projection::*;
mod error;
pub use error::*;

use veoveo_platform_store::{PlatformClient, PlatformStore};

#[derive(Clone, Debug)]
pub struct MapRepository {
    store: PlatformStore,
}
impl MapRepository {
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
