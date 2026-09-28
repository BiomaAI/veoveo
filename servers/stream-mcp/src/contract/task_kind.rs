use veoveo_types::{TaskTypeDefinition, TaskTypeName};

/// Durable operations owned by the Stream server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamTaskKind {
    RunRecording,
}

impl TaskTypeDefinition for StreamTaskKind {
    fn name(self) -> TaskTypeName {
        match self {
            Self::RunRecording => const { TaskTypeName::from_static("run_recording") },
        }
    }
}
