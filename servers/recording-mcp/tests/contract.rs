//! The server library exposes the same domain types used by ingest producers.
use veoveo_recording_mcp::{
    contract::{RecordingId, RecordingUri},
    uris,
};

#[test]
fn contract_feature_exposes_the_shared_recording_types() {
    let id = RecordingId::new();
    let uri: veoveo_recording_contract::RecordingUri = RecordingUri::new(id);
    assert_eq!(uri.id(), id);
    assert_eq!(uri, uris::recording_uri(id));
    let _: veoveo_recording_contract::RecordingResource =
        veoveo_recording_mcp::contract::RecordingResource::Recording(uri);
}
