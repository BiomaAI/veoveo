//! Recording permissions; gateway profile and resource policy grant them independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum RecordingScope {
    #[vocabulary(rename = "recording:seal")]
    Seal,
}

/// Permissions for machine producers. These do not grant Recording MCP operations.
///
/// ```compile_fail
/// use veoveo_recording_contract::{RecordingProducerScope, RecordingScope};
/// fn producer(_: RecordingProducerScope) {}
/// producer(RecordingScope::Seal);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum RecordingProducerScope {
    #[vocabulary(rename = "recording:ingest")]
    Ingest,
    #[vocabulary(rename = "recording:publish")]
    Publish,
}
