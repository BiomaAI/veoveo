//! Code-owned Task operations, independent of the database and MCP runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(task_type)]
pub enum ComputerTaskKind {
    #[vocabulary(rename = "computer.lifecycle")]
    Lifecycle,
    #[vocabulary(rename = "computer.execution")]
    Execution,
    #[vocabulary(rename = "computer.file_transfer")]
    FileTransfer,
    #[vocabulary(rename = "computer.maintenance")]
    Maintenance,
}
