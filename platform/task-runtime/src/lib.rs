//! Durable execution state for Veoveo MCP tasks.
//!
//! SurrealDB is the sole task authority. In-process notifications and LIVE
//! queries may reduce latency, but every read and transition is checked
//! against durable state. Native changefeeds record committed Task transitions.

mod admission;
mod contributions;
pub mod hosting;
mod leases;
mod mcp;
mod provider_resume;
mod provider_transaction;
mod recovery;
mod resource_subscriptions;
mod runtime;
mod service;
mod types;

pub use contributions::{
    OwnedTaskTable, TaskContribution, TaskContributions, TaskCreation, TaskSettlement,
};
pub use hosting::{DurableListener, DurableTasks, TasksOnly, WithResources};
pub use mcp::{project_snapshot, task_seed};
pub use provider_transaction::ProviderCommit;
pub use resource_subscriptions::{
    TaskResourceSubscriptions, TaskResourceUpdate, TaskResourceUpdateStream,
};
pub use runtime::{
    OwnerTaskQuery, OwnerTaskSubscription, TaskRuntime, TaskUpdateStream, TaskUsageAccess,
    TaskUsagePage,
};
pub use service::{
    DurableTaskService, DurableTaskSubscription, DurableTaskUpdateStream,
    TASK_RETENTION_PIN_META_KEY, authorized_snapshot, cancel_durable_task, durable_input_responses,
    get_durable_task, listen_durable_subscriptions, restore_task_retention_meta, retention_pins,
    start_durable_tool_task, subscribe_authorized_snapshots, subscribe_durable_tasks,
    update_durable_task,
};
pub use types::{
    ClaimedTask, CreateTask, CreateTaskResult, RecoveryClass, RecoveryReport, TaskError,
    TaskFailure, TaskInputExchange, TaskInputRequest, TaskInputSubmission, TaskOwner, TaskPage,
    TaskPageCursor, TaskPayloadState, TaskRetentionPin, TaskRetentionPinError, TaskRuntimeConfig,
    TaskSnapshot, TaskTransition, TaskUpdate, TaskUpdateCursor,
};
pub use veoveo_platform_store::{PrincipalKind, StoreAuthLevel, StoreCredentials, TaskStatus};
