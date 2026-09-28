//! Code-owned Task operations, independent of the database and MCP runtime.
veoveo_types::declare_task_types! {
    pub enum ComputerTaskKind {
        Lifecycle => "computer.lifecycle",
        Execution => "computer.execution",
        FileTransfer => "computer.file_transfer",
        Maintenance => "computer.maintenance",
    }
}
