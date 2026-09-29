//! Fixed Recording declarations, qualified against the owning typed addresses.
use crate::{RecordingDocument, RecordingId, RecordingLayersUri, RecordingResource, RecordingUri};
use veoveo_types::{ResourceAddress, ResourceUri};
pub const DOCS_URI: &str = "recording://docs";
pub const CONTRACT_URI: &str = "recording://contract";
pub const DOC_TEMPLATE: &str = "recording://docs/{doc_id}";

pub const CATALOG_URI: &str = "recording://catalog";
pub const CATALOG_TEMPLATE: &str = "recording://catalog{?cursor}";
pub const EXPLORER_APP_URI: &str = "ui://recording/explorer.html";
pub const RECORDING_TEMPLATE: &str = "recording://recordings/{recording_id}";
pub const LAYERS_TEMPLATE: &str = "recording://recordings/{recording_id}/layers";

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
