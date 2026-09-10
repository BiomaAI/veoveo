use serde::{Deserialize, Serialize};

/// A refusal can release the slot only while the journal proves no dispatch escaped.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRefusal {
    CancelledBeforeDispatch,
    AuthorityDenied,
    RunChanged,
}

/// The reason for containing an admitted run does not claim its writes rolled back.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandInterruption {
    Cancelled,
    Deadline,
    AuthorityLost,
    ExecutionUnknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandOutcome {
    Completed(crate::api::ExecutionResult),
    Undispatched(CommandRefusal),
    Terminated(CommandInterruption),
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
