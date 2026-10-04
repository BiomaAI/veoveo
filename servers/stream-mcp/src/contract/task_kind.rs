//! Code-owned Task operations, independent of the database and MCP runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(task_type)]
pub enum StreamTaskKind {
    #[vocabulary(rename = "run_recording")]
    RunRecording,
}
