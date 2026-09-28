//! Code-owned Task operations, independent of the database and MCP runtime.
veoveo_types::declare_task_types! {
    pub enum SpeechTaskKind {
        Transcribe => "transcribe",
    }
}
