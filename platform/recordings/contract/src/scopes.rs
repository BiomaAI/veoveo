//! Recording permissions; gateway profile and resource policy grant them independently.
veoveo_types::scope_enum! {
    pub enum RecordingScope {
        Seal => "recording:seal",
    }
}

veoveo_types::scope_enum! {
    /// Permissions for machine producers. These do not grant Recording MCP operations.
    ///
    /// ```compile_fail
    /// use veoveo_recording_contract::{RecordingProducerScope, RecordingScope};
    /// fn producer(_: RecordingProducerScope) {}
    /// producer(RecordingScope::Seal);
    /// ```
    pub enum RecordingProducerScope {
        Ingest => "recording:ingest",
        Publish => "recording:publish",
    }
}
