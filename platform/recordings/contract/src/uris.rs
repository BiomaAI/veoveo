//! Fixed Recording declarations, qualified against the owning typed addresses.
use crate::{RecordingDocument, RecordingId, RecordingLayersUri, RecordingResource, RecordingUri};
use veoveo_types::{ResourceAddress, ResourceUri};
pub const DOCS_URI: &str = RecordingResource::RESOURCE_TEMPLATE_DOCS;
pub const CONTRACT_URI: &str = RecordingResource::RESOURCE_TEMPLATE_CONTRACT;
pub const DOC_TEMPLATE: &str = RecordingResource::RESOURCE_TEMPLATE_DOCUMENT;

pub const CATALOG_URI: &str = "recording://catalog";
pub const CATALOG_TEMPLATE: &str = RecordingResource::RESOURCE_TEMPLATE_CATALOG;
pub const EXPLORER_APP_URI: &str = RecordingResource::RESOURCE_TEMPLATE_EXPLORER;
pub const RECORDING_TEMPLATE: &str = RecordingResource::RESOURCE_TEMPLATE_RECORDING;
pub const LAYERS_TEMPLATE: &str = RecordingResource::RESOURCE_TEMPLATE_LAYERS;

pub fn doc_uri(doc: RecordingDocument) -> ResourceUri {
    RecordingResource::Document(doc)
        .to_uri()
        .expect("declared document")
}
pub fn recording_uri(id: RecordingId) -> RecordingUri {
    RecordingUri::new(id)
}
pub fn layers_uri(id: RecordingId) -> RecordingLayersUri {
    RecordingLayersUri::new(id)
}
