#[path = "ids.rs"]
mod ids;
pub use ids::*;
#[path = "models.rs"]
mod models;
pub use models::*;
#[path = "recordings.rs"]
mod recordings;
pub use recordings::*;
#[path = "recording_catalog.rs"]
mod recording_catalog;
pub use recording_catalog::*;
#[path = "recording_ingest.rs"]
mod recording_ingest;
pub use recording_ingest::*;
#[path = "recording_blueprints.rs"]
mod recording_blueprints;
pub use recording_blueprints::*;
#[path = "error.rs"]
mod error;
pub use error::*;

use veoveo_platform_store::{PlatformClient, PlatformStore};
#[derive(Clone, Debug)]
pub struct RecordingRepository {
    store: PlatformStore,
}
impl RecordingRepository {
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

#[path = "manifest_publication.rs"]
mod manifest_publication;
pub use manifest_publication::*;

#[path = "properties_preparation.rs"]
mod properties_preparation;
pub use properties_preparation::{RecordingPropertiesPreparation, source_layer_manifest_digest};
