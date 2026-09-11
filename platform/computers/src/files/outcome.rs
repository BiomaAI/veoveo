use serde::{Deserialize, Serialize};

/// A refusal can release the slot only while the journal proves no dispatch escaped.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRefusal {
    CancelledBeforeDispatch,
    AuthorityDenied,
    RunChanged,
    PreparationExpired,
    ArtifactUnavailable,
}

/// The reason for containing an admitted run does not claim its writes rolled back.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileInterruption {
    Cancelled,
    Deadline,
    AuthorityLost,
    ExecutionUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileOutcome {
    Completed(crate::api::FileTransferResult),
    Rejected(veoveo_computer_execution::FileFailure),
    Undispatched(FileRefusal),
    Terminated(FileInterruption),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationSource {
    Stop,
    Read,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationEvidence {
    pub kind: TerminationSource,
    pub id: uuid::Uuid,
}
