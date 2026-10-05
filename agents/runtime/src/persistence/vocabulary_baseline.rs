//! SDK value/serde declarations captured before the Vocabulary migration.

use surrealdb::types as surrealdb_types;
use surrealdb::types::SurrealValue;
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum WakeKind {
    #[serde(rename = "task_result")]
    #[surreal(value = "task_result")]
    TaskResult,
    #[serde(rename = "resource_changed")]
    #[surreal(value = "resource_changed")]
    ResourceChanged,
    #[serde(rename = "timer")]
    #[surreal(value = "timer")]
    Timer,
    #[serde(rename = "operator_message")]
    #[surreal(value = "operator_message")]
    OperatorMessage,
    #[serde(rename = "input_request")]
    #[surreal(value = "input_request")]
    InputRequest,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum AgentState {
    #[serde(rename = "idle")]
    #[surreal(value = "idle")]
    Idle,
    #[serde(rename = "running")]
    #[surreal(value = "running")]
    Running,
    #[serde(rename = "waiting")]
    #[surreal(value = "waiting")]
    Waiting,
    #[serde(rename = "disabled")]
    #[surreal(value = "disabled")]
    Disabled,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum WakeState {
    #[serde(rename = "pending")]
    #[surreal(value = "pending")]
    Pending,
    #[serde(rename = "claimed")]
    #[surreal(value = "claimed")]
    Claimed,
    #[serde(rename = "acked")]
    #[surreal(value = "acked")]
    Acked,
    #[serde(rename = "coalesced")]
    #[surreal(value = "coalesced")]
    Coalesced,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum AgentEpisodeState {
    #[serde(rename = "running")]
    #[surreal(value = "running")]
    Running,
    #[serde(rename = "completed")]
    #[surreal(value = "completed")]
    Completed,
    #[serde(rename = "budget_terminated")]
    #[surreal(value = "budget_terminated")]
    BudgetTerminated,
    #[serde(rename = "stopped")]
    #[surreal(value = "stopped")]
    Stopped,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
    #[serde(rename = "crashed")]
    #[surreal(value = "crashed")]
    Crashed,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum AgentTaskWatchState {
    #[serde(rename = "pending")]
    #[surreal(value = "pending")]
    Pending,
    #[serde(rename = "watching")]
    #[surreal(value = "watching")]
    Watching,
    #[serde(rename = "resolved")]
    #[surreal(value = "resolved")]
    Resolved,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum AgentInputRequestState {
    #[serde(rename = "pending")]
    #[surreal(value = "pending")]
    Pending,
    #[serde(rename = "answered")]
    #[surreal(value = "answered")]
    Answered,
    #[serde(rename = "declined")]
    #[surreal(value = "declined")]
    Declined,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
fn compare<A, B>(actual: A, before: B, spelling: &str)
where
    A: Copy
        + std::fmt::Debug
        + PartialEq
        + SurrealValue
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + veoveo_types::Vocabulary,
    B: Copy + SurrealValue + serde::Serialize,
{
    let database = before.into_value();
    assert_eq!(actual.into_value(), database);
    assert_eq!(A::from_value(database.clone()).unwrap(), actual);
    assert_eq!(A::is_value(&database), B::is_value(&database));
    for invalid in [
        surrealdb::types::Value::String("unknown".into()),
        surrealdb::types::Value::None,
    ] {
        assert_eq!(A::is_value(&invalid), B::is_value(&invalid));
        assert_eq!(
            A::from_value(invalid.clone()).is_err(),
            B::from_value(invalid).is_err()
        );
    }
    let wire = serde_json::to_value(before).unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), wire);
    assert_eq!(serde_json::from_value::<A>(wire).unwrap(), actual);
    assert_eq!(actual.as_str(), spelling);
}
#[test]
fn database_vocabulary_values_preserve_the_published_profile() {
    assert_eq!(super::AgentState::kind_of(), AgentState::kind_of());
    compare(super::AgentState::Idle, AgentState::Idle, "idle");
    compare(super::AgentState::Running, AgentState::Running, "running");
    compare(super::AgentState::Waiting, AgentState::Waiting, "waiting");
    compare(
        super::AgentState::Disabled,
        AgentState::Disabled,
        "disabled",
    );
    compare(super::AgentState::Failed, AgentState::Failed, "failed");
    assert_eq!(super::WakeState::kind_of(), WakeState::kind_of());
    compare(super::WakeState::Pending, WakeState::Pending, "pending");
    compare(super::WakeState::Claimed, WakeState::Claimed, "claimed");
    compare(super::WakeState::Acked, WakeState::Acked, "acked");
    compare(
        super::WakeState::Coalesced,
        WakeState::Coalesced,
        "coalesced",
    );
    compare(super::WakeState::Failed, WakeState::Failed, "failed");
    assert_eq!(
        super::AgentEpisodeState::kind_of(),
        AgentEpisodeState::kind_of()
    );
    compare(
        super::AgentEpisodeState::Running,
        AgentEpisodeState::Running,
        "running",
    );
    compare(
        super::AgentEpisodeState::Completed,
        AgentEpisodeState::Completed,
        "completed",
    );
    compare(
        super::AgentEpisodeState::BudgetTerminated,
        AgentEpisodeState::BudgetTerminated,
        "budget_terminated",
    );
    compare(
        super::AgentEpisodeState::Stopped,
        AgentEpisodeState::Stopped,
        "stopped",
    );
    compare(
        super::AgentEpisodeState::Failed,
        AgentEpisodeState::Failed,
        "failed",
    );
    compare(
        super::AgentEpisodeState::Crashed,
        AgentEpisodeState::Crashed,
        "crashed",
    );
    assert_eq!(
        super::AgentTaskWatchState::kind_of(),
        AgentTaskWatchState::kind_of()
    );
    compare(
        super::AgentTaskWatchState::Pending,
        AgentTaskWatchState::Pending,
        "pending",
    );
    compare(
        super::AgentTaskWatchState::Watching,
        AgentTaskWatchState::Watching,
        "watching",
    );
    compare(
        super::AgentTaskWatchState::Resolved,
        AgentTaskWatchState::Resolved,
        "resolved",
    );
    compare(
        super::AgentTaskWatchState::Failed,
        AgentTaskWatchState::Failed,
        "failed",
    );
    compare(
        super::AgentTaskWatchState::Cancelled,
        AgentTaskWatchState::Cancelled,
        "cancelled",
    );
    assert_eq!(
        super::AgentInputRequestState::kind_of(),
        AgentInputRequestState::kind_of()
    );
    compare(
        super::AgentInputRequestState::Pending,
        AgentInputRequestState::Pending,
        "pending",
    );
    compare(
        super::AgentInputRequestState::Answered,
        AgentInputRequestState::Answered,
        "answered",
    );
    compare(
        super::AgentInputRequestState::Declined,
        AgentInputRequestState::Declined,
        "declined",
    );
    compare(
        super::AgentInputRequestState::Cancelled,
        AgentInputRequestState::Cancelled,
        "cancelled",
    );
    assert_eq!(super::WakeKind::kind_of(), WakeKind::kind_of());
    compare(
        super::WakeKind::TaskResult,
        WakeKind::TaskResult,
        "task_result",
    );
    compare(
        super::WakeKind::ResourceChanged,
        WakeKind::ResourceChanged,
        "resource_changed",
    );
    compare(super::WakeKind::Timer, WakeKind::Timer, "timer");
    compare(
        super::WakeKind::OperatorMessage,
        WakeKind::OperatorMessage,
        "operator_message",
    );
    compare(
        super::WakeKind::InputRequest,
        WakeKind::InputRequest,
        "input_request",
    );
}
