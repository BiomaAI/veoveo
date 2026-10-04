//! Code-owned Task operations, independent of the database and MCP runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(task_type)]
pub enum TimeTaskKind {
    #[vocabulary(rename = "expand_schedule")]
    ExpandSchedule,
    #[vocabulary(rename = "validate_timeline")]
    ValidateTimeline,
}
