//! Code-owned Task operations, independent of the database and MCP runtime.
veoveo_types::declare_task_types! {
    pub enum FramesTaskKind {
        BatchTransform => "batch_transform",
    }
}
