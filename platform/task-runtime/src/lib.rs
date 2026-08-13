//! Durable execution state for Veoveo MCP tasks.
//!
//! SurrealDB is the sole task authority. In-process notifications and LIVE
//! queries may reduce latency, but every read and transition is checked
//! against durable state and every state transition emits an ordered outbox
//! event in the same transaction.

mod mcp;
mod runtime;
mod service;
mod types;

pub use mcp::{project_snapshot, task_seed};
pub use runtime::{TaskRuntime, TaskUpdateStream};
pub use service::{
    DurableTaskService, DurableTaskSubscription, DurableTaskUpdateStream,
    TASK_RETENTION_PIN_META_KEY, authorized_snapshot, cancel_durable_task, durable_input_responses,
    get_durable_task, listen_durable_subscriptions, retention_pins, subscribe_durable_tasks,
    update_durable_task,
};
pub use types::{
    ClaimedTask, CreateTask, CreateTaskResult, RecoveryClass, RecoveryReport, TaskError,
    TaskFailure, TaskInputExchange, TaskInputRequest, TaskInputSubmission, TaskOwner,
    TaskPayloadState, TaskRetentionPin, TaskRetentionPinError, TaskRuntimeConfig, TaskSnapshot,
    TaskTransition, TaskUpdate, TaskUpdateCursor,
};
pub use veoveo_platform_store::{
    PrincipalKind, StoreAuthLevel, StoreCredentials, TaskId, TaskStatus,
};
