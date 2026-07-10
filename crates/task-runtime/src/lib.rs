//! Durable execution state for Veoveo MCP tasks.
//!
//! SurrealDB is the sole task authority. In-process notifications and LIVE
//! queries may reduce latency, but every read and transition is checked
//! against durable state and every state transition emits an ordered outbox
//! event in the same transaction.

mod runtime;
mod types;

pub use runtime::{TaskRuntime, TaskUpdateStream};
pub use types::{
    ClaimedTask, CreateTask, CreateTaskResult, RecoveryClass, RecoveryReport, TaskError,
    TaskFailure, TaskInputExchange, TaskInputRequest, TaskInputSubmission, TaskOwner,
    TaskPayloadState, TaskRetentionPin, TaskRetentionPinError, TaskRuntimeConfig, TaskSnapshot,
    TaskTransition, TaskUpdate, TaskUpdateCursor,
};
pub use veoveo_platform_store::{
    PrincipalKind, StoreAuthLevel, StoreCredentials, TaskId, TaskStatus,
};
