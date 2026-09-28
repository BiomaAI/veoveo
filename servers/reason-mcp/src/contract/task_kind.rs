use veoveo_types::{TaskTypeDefinition, TaskTypeName};

/// Durable operations owned by the Reason server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReasonTaskKind {
    AnalyzeRecording,
}

impl TaskTypeDefinition for ReasonTaskKind {
    fn name(self) -> TaskTypeName {
        match self {
            Self::AnalyzeRecording => const { TaskTypeName::from_static("analyze_recording") },
        }
    }
}
