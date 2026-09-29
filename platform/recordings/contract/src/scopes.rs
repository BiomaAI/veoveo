//! Recording permissions; gateway profile and resource policy grant them independently.
veoveo_types::scope_enum! {
    pub enum RecordingScope {
        Seal => "recording:seal",
    }
}
