//! Selected schedule Task acceptance over the maintained installed SDK/lifecycle.
use super::cleanup;
use super::{installed, open_receipt, trace};
#[path = "schedule/handoff.rs"]
mod handoff;
#[path = "schedule/lifecycle.rs"]
mod lifecycle;
#[cfg(test)]
#[path = "schedule/workload.rs"]
mod workload;
use anyhow::{Context, Result, ensure};
use rmcp::{
    model::{
        DetailedTask, GetMeta, ServerNotification, SubscriptionFilter, Task, TaskPayload,
        TaskStatus,
    },
    service::Subscription,
};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use veoveo_testing_support::{
    SmokeMcpClient, await_task_terminal_with_timeout, call_tool_as_task, lifecycle::owner,
    task_payload,
};
use veoveo_time_mcp::{
    EffectiveTimeAuthority, ExpandScheduleOutput, ExpandScheduleRequest, TimeResource,
};
use veoveo_types::CanonicalTaskId;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    installation: installed::InstalledSource,
    authority: EffectiveTimeAuthority,
    request: ExpandScheduleRequest,
    expected: ExpandScheduleOutput,
    mode: lifecycle::Mode,
    recovery: Option<lifecycle::Fixture>,
}
impl Input {
    fn admit(&self) -> Result<()> {
        lifecycle::admit(self)?;
        ensure!(
            (1..=512).contains(&self.request.maximum_occurrences),
            "schedule fixture occurrence limit must be 1..=512"
        );
        ensure!(
            (1..=8).contains(&self.request.calendar.windows.len()),
            "schedule fixture requires one to eight calendar windows"
        );
        ensure!(
            self.request.calendar.windows.iter().all(|window| (1..=31)
                .contains(&window.recurrence.interval)
                && window
                    .recurrence
                    .count
                    .is_some_and(|count| (1..=1_000_000).contains(&count))),
            "schedule fixture recurrence requires count 1..=1000000 and interval 1..=31"
        );
        ensure!(
            self.request.horizon.authority() == &self.authority.binding(),
            "schedule horizon must bind selected authority"
        );
        ensure!(
            self.request.horizon.end().total_nanoseconds()
                - self.request.horizon.start().total_nanoseconds()
                <= 31 * 86_400 * 1_000_000_000i128,
            "schedule horizon must span at most 31 days"
        );
        ensure!(
            !self.expected.occurrences.is_empty()
                && self.expected.occurrences.len() <= self.request.maximum_occurrences as usize,
            "schedule fixture requires nonempty bounded independent expectations"
        );
        ensure!(
            !self.expected.truncated,
            "schedule acceptance requires an untruncated expected output"
        );
        for (index, occurrence) in self.expected.occurrences.iter().enumerate() {
            ensure!(
                occurrence.sequence as usize == index,
                "schedule expectation sequence disagrees"
            );
            ensure!(
                occurrence.window.authority() == self.request.horizon.authority()
                    && occurrence.window.start().total_nanoseconds()
                        >= self.request.horizon.start().total_nanoseconds()
                    && occurrence.window.end().total_nanoseconds()
                        <= self.request.horizon.end().total_nanoseconds(),
                "schedule expectation must lie inside selected horizon and authority"
            );
            ensure!(
                index == 0
                    || self.expected.occurrences[index - 1]
                        .window
                        .start()
                        .total_nanoseconds()
                        <= occurrence.window.start().total_nanoseconds(),
                "schedule expectations must be ordered"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Phase {
    Admitted,
    Preconditions,
    DispatchIntent,
    Created,
    Listening,
    Working,
    CancelIntent,
    CancelAcknowledged,
    CrashAdmission,
    CrashArmIntent,
    CrashArmed,
    ReplacementObserved,
    ReplacementListening,
    DeliveredCancelled,
    DeliveredCompleted,
    CurrentState,
    Result,
    Passed,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Failure {
    OwnerLifecycle,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskObservation {
    task_id: CanonicalTaskId,
    status: TaskStatus,
    created_at: chrono::DateTime<chrono::Utc>,
    last_updated_at: chrono::DateTime<chrono::Utc>,
}
impl TaskObservation {
    fn admit(task: &Task) -> Result<Self> {
        Ok(Self {
            task_id: CanonicalTaskId::parse(&task.task_id)?,
            status: task.status,
            created_at: task.created_at.parse()?,
            last_updated_at: task.last_updated_at.parse()?,
        })
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Journal<'a> {
    schema_version: &'static str,
    mode: lifecycle::Mode,
    lifecycle: lifecycle::Evidence,
    authority: &'a EffectiveTimeAuthority,
    request: &'a ExpandScheduleRequest,
    expected: &'a ExpandScheduleOutput,
    phase: Phase,
    dispatch_intent: bool,
    task_id: Option<CanonicalTaskId>,
    created: Option<TaskObservation>,
    first_delivered_status: Option<TaskStatus>,
    working_delivered: bool,
    delivered_completed: Option<TaskObservation>,
    current: Option<TaskObservation>,
    result: Option<ExpandScheduleOutput>,
    terminal_settled: bool,
    listener_closed: bool,
    caller_closed: bool,
    failure: Option<Failure>,
    trace: trace::Trace,
    cleanup_trace: trace::Trace,
    cleanup_owner: &'static str,
    remaining_gates: Vec<&'static str>,
}
impl<'a> Journal<'a> {
    fn new(input: &'a Input) -> Self {
        Self {
            schema_version: "veoveo.ai/time-schedule-task-acceptance/v2",
            mode: input.mode,
            lifecycle: lifecycle::Evidence::default(),
            authority: &input.authority,
            request: &input.request,
            expected: &input.expected,
            phase: Phase::Admitted,
            dispatch_intent: false,
            task_id: None,
            created: None,
            first_delivered_status: None,
            working_delivered: false,
            delivered_completed: None,
            current: None,
            result: None,
            terminal_settled: false,
            listener_closed: false,
            caller_closed: false,
            failure: None,
            trace: trace::Trace::default(),
            cleanup_trace: trace::Trace::default(),
            cleanup_owner: "veoveo-testing-support/lifecycle/owner",
            remaining_gates: vec![
                "schedule_task_cancellation",
                "unfinished_schedule_task_restart_recovery",
                "guaranteed_working_to_completed_subscription_transition",
                "authority_activation_and_replica_races",
            ],
        }
    }
    fn persist(&self, output: &mut fs::File) -> Result<()> {
        // Append-only snapshots retain intent and identity across interrupted later requests.
        serde_json::to_writer(&mut *output, self)?;
        output.write_all(b"\n")?;
        output.sync_all()?;
        Ok(())
    }
}

struct Handles {
    caller: Option<SmokeMcpClient>,
    listener: Option<Subscription>,
    task_id: Option<CanonicalTaskId>,
    caller_closed: bool,
    listener_closed: bool,
    trace: trace::Trace,
    caller_close: cleanup::CloseState,
    listener_close: cleanup::CloseState,
    replacement_listener: Option<Subscription>,
    replacement_close: cleanup::CloseState,
    crash_watch: Option<veoveo_testing_support::installed::restart::CrashWatch>,
    watch_close: cleanup::CloseState,
    crash_receipt: Option<veoveo_testing_support::installed::restart::CrashReceipt>,
}
impl Handles {
    fn new() -> Self {
        Self {
            caller: None,
            listener: None,
            task_id: None,
            caller_closed: true,
            listener_closed: true,
            trace: trace::Trace::default(),
            caller_close: cleanup::CloseState::default(),
            listener_close: cleanup::CloseState::default(),
            replacement_listener: None,
            replacement_close: cleanup::CloseState::default(),
            crash_watch: None,
            watch_close: cleanup::CloseState::default(),
            crash_receipt: None,
        }
    }
}

#[tokio::test]
#[ignore = "requires installed Time, Task-enabled OAuth caller, stable selected authority/calendar, independent schedule expectation and a new private receipt path"]
async fn typed_schedule_task_through_public_gateway() -> Result<()> {
    let input: Input = installed::input_from("VEOVEO_TIME_SCHEDULE_TASK_INPUT")
        .map_err(|_| anyhow::anyhow!("Time schedule fixture input admission failed"))?;
    input
        .admit()
        .map_err(|_| anyhow::anyhow!("Time schedule fixture preconditions failed"))?;
    let target = input
        .installation
        .validate()
        .map_err(|_| anyhow::anyhow!("Time schedule installation admission failed"))?;
    let mut output = open_receipt(&input.installation.output)
        .map_err(|_| anyhow::anyhow!("Time schedule receipt admission failed"))?;
    let mut journal = Journal::new(&input);
    journal
        .persist(&mut output)
        .map_err(|_| anyhow::anyhow!("Time schedule receipt persistence failed"))?;
    // Keep actual SDK handles outside work that deadlines or signals can drop.
    let handles = Arc::new(Mutex::new(Handles::new()));
    let result = owner::run(async {
        let deadline = tokio::time::Instant::now() + input.mode.operation_budget();
        let cleanup_handles = handles.clone();
        owner::register_cleanup(
            owner::CleanupKind::Remote,
            "Time schedule SDK handles",
            &uuid::Uuid::now_v7().to_string(),
            move || async move { close_handles(&mut *cleanup_handles.lock().await).await },
        )?;

        let mut owned = handles.lock().await;
        journal.trace.begin(trace::Request::TaskConnection);
        let caller = tokio::time::timeout_at(
            deadline.min(tokio::time::Instant::now() + Duration::from_secs(75)),
            input.installation.task_caller(),
        )
        .await
        .context("Time schedule connection deadline")
        .and_then(|value| value);
        let caller = match caller {
            Ok(caller) => {
                journal.trace.finish(&Ok::<(), anyhow::Error>(()));
                caller
            }
            Err(error) => {
                journal.trace.failure(&error);
                anyhow::bail!("Task connection failed");
            }
        };
        owned.caller = Some(caller);
        owned.caller_closed = false;
        let exercise = tokio::time::timeout_at(
            deadline,
            exercise(
                &mut owned,
                &input,
                &target,
                &mut journal,
                &mut output,
                deadline,
            ),
        )
        .await
        .context("Time schedule exercise deadline")
        .and_then(|value| value);
        if let Err(error) = &exercise {
            journal.trace.interrupt(error);
        }
        exercise
    })
    .await;
    if let Err(error) = &result {
        journal.lifecycle.failure(error);
        journal.trace.interrupt(error);
    }
    // The registered action ran within the owner's one cleanup grace after work dropped.
    let mut owned = handles.lock().await;
    journal.listener_closed = owned.listener_closed;
    journal.caller_closed = owned.caller_closed;
    journal.cleanup_trace = std::mem::take(&mut owned.trace);
    journal.lifecycle.watch = owned.crash_receipt.clone();
    journal.lifecycle.watch_closed = owned.crash_watch.is_none() && owned.watch_close.closed();
    journal.lifecycle.replacement_listener_closed =
        owned.replacement_listener.is_none() && owned.replacement_close.closed();
    drop(owned);
    if result.is_err()
        || !journal.listener_closed
        || !journal.caller_closed
        || !journal.lifecycle.watch_closed
        || !journal.lifecycle.replacement_listener_closed
    {
        journal.phase = Phase::Failed;
        journal.failure = Some(Failure::OwnerLifecycle);
    } else {
        journal.phase = Phase::Passed;
    }
    journal
        .persist(&mut output)
        .map_err(|_| anyhow::anyhow!("Time schedule receipt persistence failed"))?;
    ensure!(
        journal.failure.is_none(),
        "Time schedule acceptance failed; inspect private receipt and lifecycle ownership lease"
    );
    Ok(())
}

async fn close_handles(handles: &mut Handles) -> Result<()> {
    let deadline = owner::cleanup_deadline()?;
    if let Some(watch) = &handles.crash_watch {
        handles.crash_receipt = watch.snapshot();
    }
    let watch = handles
        .watch_close
        .close(
            &mut handles.crash_watch,
            |watch| async move {
                ensure!(watch.close().await, "Time crash watch close failed");
                Ok(())
            },
            deadline,
        )
        .await;
    let replacement =
        handles
            .replacement_close
            .close(
                &mut handles.replacement_listener,
                |mut subscription| async move {
                    subscription.cancel().await.map_err(anyhow::Error::from)
                },
                deadline,
            )
            .await;
    if !handles.listener_closed {
        if let Some(id) = handles.task_id.as_ref() {
            handles
                .trace
                .begin(trace::Request::TaskSubscriptionClose(id.clone()));
        }
        let closed = handles
            .listener_close
            .close(
                &mut handles.listener,
                |mut subscription| async move {
                    subscription.cancel().await.map_err(anyhow::Error::from)
                },
                owner::cleanup_deadline()?,
            )
            .await;
        handles.listener_closed = closed.is_ok();
        handles.trace.finish(&closed);
    }
    // A failed listener close does not suppress the caller close's remaining allowance.
    if !handles.caller_closed {
        handles.trace.begin(trace::Request::TaskConnectionClose);
        let closed = handles
            .caller_close
            .close(
                &mut handles.caller,
                |connection| connection.cancel(),
                owner::cleanup_deadline()?,
            )
            .await;
        handles.caller_closed = closed.is_ok();
        handles.trace.finish(&closed);
    }
    watch?;
    replacement?;
    ensure!(
        handles.listener_closed && handles.caller_closed,
        "Time schedule SDK handle cleanup remains unresolved"
    );
    Ok(())
}

#[cfg(test)]
async fn close_slot<H, F: std::future::Future<Output = Result<()>>>(
    slot: &mut Option<H>,
    close: impl FnOnce(H) -> F,
    deadline: std::time::Instant,
) -> Result<()> {
    let allowance =
        Duration::from_secs(10).min(deadline.saturating_duration_since(std::time::Instant::now()));
    ensure!(
        !allowance.is_zero(),
        "Time schedule SDK handle cleanup grace exhausted"
    );
    let handle = slot
        .take()
        .context("Time schedule SDK handle close previously interrupted")?;
    tokio::time::timeout(allowance, close(handle))
        .await
        .context("Time schedule SDK handle cleanup deadline")?
}

async fn exercise(
    handles: &mut Handles,
    input: &Input,
    target: &veoveo_deploy_contract::InstallationTarget,
    journal: &mut Journal<'_>,
    output: &mut fs::File,
    deadline: tokio::time::Instant,
) -> Result<()> {
    let caller = handles.caller.as_ref().expect("admitted Task caller");
    ensure!(
        caller
            .peer_info()
            .is_some_and(|info| info.capabilities.supports_tasks()),
        "Time does not advertise official Tasks"
    );
    let authority: EffectiveTimeAuthority = journal
        .trace
        .read(caller.peer(), TimeResource::AuthoritiesCurrent)
        .await?;
    ensure!(
        authority == input.authority,
        "schedule current authority differs"
    );
    let calendar: veoveo_time_mcp::OperationalCalendar = journal
        .trace
        .read(
            caller.peer(),
            TimeResource::Calendar {
                id: input.request.calendar.calendar_id.clone(),
                version: input.request.calendar.version,
            },
        )
        .await?;
    ensure!(
        calendar == input.request.calendar,
        "selected schedule calendar differs"
    );
    let driver = lifecycle::admit_target(input, target, caller, journal, output, deadline).await?;
    journal.phase = Phase::Preconditions;
    journal.persist(output)?;
    journal.phase = Phase::DispatchIntent;
    journal.dispatch_intent = true;
    journal
        .trace
        .begin(trace::Request::ScheduleCreate(input.request.clone()));
    journal.persist(output)?;
    let created = call_tool_as_task(
        caller,
        "time__expand_schedule",
        serde_json::to_value(&input.request)?,
    )
    .await;
    journal.trace.finish(&created);
    let created = created?;
    let id = CanonicalTaskId::parse(&created.task_id)?;
    handles.task_id = Some(id.clone());
    journal.task_id = Some(id.clone());
    journal.phase = Phase::Created;
    // Retain acknowledged identity even if later response metadata fails admission.
    journal.persist(output)?;
    journal.created = Some(TaskObservation::admit(&created)?);
    journal.persist(output)?;
    let filter = SubscriptionFilter::builder()
        .task_id(id.to_string())
        .build();
    journal.trace.begin(trace::Request::TaskListen(id.clone()));
    let subscription = match caller.listen(filter.clone()).await {
        Ok(subscription) => {
            journal.trace.finish(&Ok::<(), anyhow::Error>(()));
            subscription
        }
        Err(error) => {
            journal.trace.failure(&anyhow::Error::from(error));
            anyhow::bail!("Task listen failed");
        }
    };
    handles.listener = Some(subscription);
    handles.listener_closed = false;
    if input.mode != lifecycle::Mode::Complete {
        lifecycle::observe(
            input,
            driver.as_ref(),
            &created,
            handles,
            journal,
            output,
            deadline,
        )
        .await?;
    }
    let caller = handles.caller.as_ref().expect("retained Task caller");
    let subscription = if input.mode == lifecycle::Mode::Recover {
        handles.replacement_listener.as_mut()
    } else {
        handles.listener.as_mut()
    }
    .context("retained terminal listener")?;
    let observed = async {
        ensure!(
            subscription.acknowledged() == &filter,
            "exact Task subscription was not acknowledged"
        );
        journal.phase = Phase::Listening;
        journal.persist(output)?;
        journal
            .trace
            .begin(trace::Request::TaskDelivery(id.clone()));
        tokio::time::timeout(
            Duration::from_secs(60),
            delivered(subscription, &id, journal, output),
        )
        .await
        .context("completed Task delivery deadline")?
    }
    .await;
    if let Err(error) = &observed {
        journal.trace.failure(error);
    }
    let delivered = observed?;
    journal.trace.begin(trace::Request::TaskGet(id.clone()));
    let current =
        await_task_terminal_with_timeout(caller, id.as_str(), Duration::from_secs(15)).await;
    journal.trace.finish(&current);
    let current = current?;
    journal.terminal_settled = true;
    journal.current = Some(TaskObservation::admit(&current.task)?);
    journal.phase = Phase::CurrentState;
    journal.persist(output)?;
    if input.mode == lifecycle::Mode::Cancel {
        lifecycle::cancelled(&id, &created, &delivered, &current)?;
        journal
            .remaining_gates
            .retain(|gate| *gate != "schedule_task_cancellation");
        return Ok(());
    }
    journal.trace.begin(trace::Request::TaskResult(id.clone()));
    let result = task_payload(caller, id.as_str()).await;
    journal.trace.finish(&result);
    let result = result?;
    let checked = (|| {
        let result = decode_output(result)?;
        assert_agreement(
            &id,
            &created,
            &delivered,
            &current,
            &result,
            &input.expected,
        )?;
        Ok(result)
    })();
    journal.trace.finish(&checked);
    let result = checked?;
    journal.result = Some(result);
    journal.phase = Phase::Result;
    journal.persist(output)?;
    let authority: EffectiveTimeAuthority = journal
        .trace
        .read(caller.peer(), TimeResource::AuthoritiesCurrent)
        .await?;
    ensure!(
        authority == input.authority,
        "authority changed during schedule qualification"
    );
    if let Some(watch) = handles.crash_watch.as_mut() {
        watch.admit_recovered_target().await?;
        journal.lifecycle.watch = watch.snapshot();
        handles.crash_receipt = journal.lifecycle.watch.clone();
        journal.lifecycle.final_target_checked = true;
        journal
            .remaining_gates
            .retain(|gate| *gate != "unfinished_schedule_task_restart_recovery");
    }
    if journal.working_delivered {
        journal
            .remaining_gates
            .retain(|gate| *gate != "guaranteed_working_to_completed_subscription_transition");
    }
    Ok(())
}

async fn delivered(
    subscription: &mut Subscription,
    id: &CanonicalTaskId,
    journal: &mut Journal<'_>,
    output: &mut fs::File,
) -> Result<DetailedTask> {
    for _ in 0..32 {
        let notification = subscription
            .next()
            .await?
            .context("Task subscription ended before Completed delivery")?;
        ensure!(
            notification.get_meta().subscription_id() == Some(subscription.id().clone()),
            "Task notification subscription identity differs"
        );
        let ServerNotification::TaskStatusNotification(update) = notification else {
            anyhow::bail!("unexpected notification in exact Task subscription");
        };
        let task = update.params.task;
        ensure!(
            CanonicalTaskId::parse(&task.task.task_id)? == *id,
            "delivered Task identity differs"
        );
        journal.first_delivered_status.get_or_insert(task.status());
        match task.status() {
            TaskStatus::Working => {
                journal.working_delivered = true;
                journal.persist(output)?;
            }
            TaskStatus::Cancelled if journal.mode == lifecycle::Mode::Cancel => {
                journal.phase = Phase::DeliveredCancelled;
                journal.persist(output)?;
                return Ok(task);
            }
            TaskStatus::Completed if journal.mode != lifecycle::Mode::Cancel => {
                journal.trace.finish(&Ok::<_, anyhow::Error>(&task));
                journal.delivered_completed = Some(TaskObservation::admit(&task.task)?);
                journal.phase = Phase::DeliveredCompleted;
                journal.persist(output)?;
                return Ok(task);
            }
            _ => anyhow::bail!("schedule Task did not complete successfully"),
        }
    }
    anyhow::bail!("Task delivery exceeded 32 notifications")
}
fn decode_output(result: rmcp::model::CallToolResult) -> Result<ExpandScheduleOutput> {
    ensure!(
        result.is_error != Some(true),
        "schedule Task returned a tool error"
    );
    serde_json::from_value(
        result
            .structured_content
            .context("schedule Task omitted structured output")?,
    )
    .context("schedule Task output admission failed")
}
fn completed_output(task: &DetailedTask) -> Result<ExpandScheduleOutput> {
    let TaskPayload::Completed { result } = &task.payload else {
        anyhow::bail!("expected completed schedule Task payload");
    };
    decode_output(serde_json::from_value(serde_json::Value::Object(
        result.clone(),
    ))?)
}
fn assert_agreement(
    id: &CanonicalTaskId,
    created: &Task,
    delivered: &DetailedTask,
    current: &DetailedTask,
    result: &ExpandScheduleOutput,
    expected: &ExpandScheduleOutput,
) -> Result<()> {
    ensure!(
        CanonicalTaskId::parse(&created.task_id)? == *id
            && CanonicalTaskId::parse(&delivered.task.task_id)? == *id
            && CanonicalTaskId::parse(&current.task.task_id)? == *id,
        "schedule Task identities disagree"
    );
    ensure!(
        created.created_at == current.task.created_at && delivered == current,
        "delivered and current Task state disagree"
    );
    ensure!(
        completed_output(delivered)? == *expected
            && completed_output(current)? == *expected
            && result == expected,
        "schedule Task results differ from independent expectation"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_time_mcp::{ScheduleOccurrence, TimeInstant, TimeWindow};

    pub(super) fn fixture() -> Result<Input> {
        let base = super::super::fixture_value();
        let authority: EffectiveTimeAuthority = serde_json::from_value(base["authority"].clone())?;
        let instant = |seconds: i128| {
            TimeInstant::from_total_nanoseconds(seconds * 1_000_000_000, 0, authority.binding())
        };
        let horizon = TimeWindow::new(instant(0)?, instant(10)?)?;
        let expected = ExpandScheduleOutput {
            occurrences: vec![ScheduleOccurrence {
                sequence: 0,
                window: TimeWindow::new(instant(1)?, instant(5)?)?,
                labels: vec![],
            }],
            truncated: false,
        };
        serde_json::from_value(serde_json::json!({
            "mode":"complete", "installation":base["installation"], "authority":authority,
            "request":{"calendar":base["calendar"], "horizon":horizon,"maximumOccurrences":4},
            "expected":expected
        }))
        .map_err(anyhow::Error::from)
    }

    #[test]
    fn schedule_fixture_rejects_unknown_fields_and_unqualified_expectations() -> Result<()> {
        let input = fixture()?;
        input.admit()?;
        let original = serde_json::json!({
            "mode":"complete", "installation":super::super::fixture_value()["installation"],
            "authority":input.authority,"request":input.request,"expected":input.expected
        });
        let mut missing_mode = original.clone();
        missing_mode.as_object_mut().unwrap().remove("mode");
        ensure!(serde_json::from_value::<Input>(missing_mode).is_err());
        let mut unknown_mode = original.clone();
        unknown_mode["mode"] = "restart".into();
        ensure!(serde_json::from_value::<Input>(unknown_mode).is_err());
        let mut unknown = original.clone();
        unknown["unexpected"] = true.into();
        ensure!(serde_json::from_value::<Input>(unknown).is_err());
        for altered in [
            {
                let mut value = original.clone();
                value["expected"]["occurrences"] = serde_json::json!([]);
                value
            },
            {
                let mut value = original.clone();
                value["expected"]["truncated"] = true.into();
                value
            },
            {
                let mut value = original.clone();
                value["expected"]["occurrences"][0]["sequence"] = 2.into();
                value
            },
            {
                let mut value = original.clone();
                value["request"]["maximumOccurrences"] = 0.into();
                value
            },
            {
                let mut value = original.clone();
                value["request"]["calendar"]["windows"][0]["recurrence"]["count"] =
                    serde_json::Value::Null;
                value
            },
        ] {
            ensure!(
                serde_json::from_value::<Input>(altered)
                    .map_or(true, |fixture| fixture.admit().is_err())
            );
        }
        let mut outside = fixture()?;
        let binding = outside.authority.binding();
        outside.expected.occurrences[0].window = TimeWindow::new(
            TimeInstant::from_total_nanoseconds(9_000_000_000, 0, binding.clone())?,
            TimeInstant::from_total_nanoseconds(11_000_000_000, 0, binding)?,
        )?;
        ensure!(outside.admit().is_err());
        Ok(())
    }

    pub(super) fn completed(
        id: &CanonicalTaskId,
        output: &ExpandScheduleOutput,
    ) -> Result<DetailedTask> {
        let result = rmcp::model::CallToolResult::structured(serde_json::to_value(output)?);
        let result = serde_json::to_value(result)?
            .as_object()
            .context("tool result object")?
            .clone();
        Ok(DetailedTask::new(
            Task::new(
                id.to_string(),
                TaskStatus::Completed,
                "2026-10-09T00:00:00Z",
                "2026-10-09T00:00:01Z",
            ),
            TaskPayload::Completed { result },
        ))
    }

    #[test]
    fn task_agreement_rejects_wrong_gateway_identity_state_and_independent_result() -> Result<()> {
        let input = fixture()?;
        let id = CanonicalTaskId::parse("gateway-schedule-known")?;
        let delivered = completed(&id, &input.expected)?;
        let mut created = delivered.task.clone();
        created.status = TaskStatus::Working;
        assert_agreement(
            &id,
            &created,
            &delivered,
            &delivered,
            &input.expected,
            &input.expected,
        )?;
        let mut current = delivered.clone();
        current.task.task_id = "gateway-other-task".into();
        ensure!(
            assert_agreement(
                &id,
                &created,
                &delivered,
                &current,
                &input.expected,
                &input.expected
            )
            .is_err()
        );
        let mut current = delivered.clone();
        current.task.last_updated_at = "2026-10-09T00:00:02Z".into();
        ensure!(
            assert_agreement(
                &id,
                &created,
                &delivered,
                &current,
                &input.expected,
                &input.expected
            )
            .is_err()
        );
        let mut incorrect = input.expected.clone();
        incorrect.occurrences[0].labels.push("unexpected".into());
        let wrong = completed(&id, &incorrect)?;
        ensure!(
            assert_agreement(&id, &created, &wrong, &wrong, &incorrect, &input.expected).is_err()
        );
        ensure!(
            decode_output(rmcp::model::CallToolResult::structured_error(
                serde_json::to_value(&input.expected)?
            ))
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn journal_retains_intent_and_opaque_identity_in_private_append_only_snapshots() -> Result<()> {
        let input = fixture()?;
        let root = tempfile::tempdir()?;
        let path = root.path().join("schedule.jsonl");
        let mut output = open_receipt(&path)?;
        let mut journal = Journal::new(&input);
        journal.phase = Phase::DispatchIntent;
        journal.dispatch_intent = true;
        journal.persist(&mut output)?;
        let id = CanonicalTaskId::parse("gateway-schedule-retained")?;
        journal.task_id = Some(id.clone());
        journal.phase = Phase::Created;
        journal.persist(&mut output)?;
        let snapshots: Vec<serde_json::Value> = fs::read_to_string(&path)?
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?;
        ensure!(
            snapshots.len() == 2
                && snapshots[0]["dispatchIntent"] == true
                && snapshots[0]["taskId"].is_null()
        );
        ensure!(snapshots[1]["taskId"] == id.as_str());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(fs::metadata(&path)?.permissions().mode() & 0o077 == 0);
        }
        ensure!(open_receipt(&path).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn timed_out_work_runs_registered_cleanup_of_retained_slots() -> Result<()> {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct DropWatch(Arc<AtomicBool>);
        impl Drop for DropWatch {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let handle_dropped = Arc::new(AtomicBool::new(false));
        let work_dropped = Arc::new(AtomicBool::new(false));
        let closed = Arc::new(AtomicBool::new(false));
        let retained = Arc::new(Mutex::new(Some(DropWatch(handle_dropped.clone()))));
        let result = owner::run(async {
            let cleanup_slot = retained.clone();
            let cleanup_closed = closed.clone();
            let cleanup_work = work_dropped.clone();
            owner::register_cleanup(
                owner::CleanupKind::Remote,
                "Time SDK slot native control",
                &uuid::Uuid::now_v7().to_string(),
                move || async move {
                    ensure!(
                        cleanup_work.load(Ordering::SeqCst),
                        "cleanup ran before pending work dropped"
                    );
                    let mut slot = cleanup_slot.lock().await;
                    close_slot(
                        &mut slot,
                        |handle| async move {
                            tokio::task::yield_now().await;
                            cleanup_closed.store(true, Ordering::SeqCst);
                            drop(handle);
                            Ok(())
                        },
                        owner::cleanup_deadline()?,
                    )
                    .await
                },
            )?;
            let work = async {
                let _watch = DropWatch(work_dropped.clone());
                let slot = retained.lock().await;
                let handle = slot.as_ref().context("retained handle")?;
                std::future::pending::<()>().await;
                std::hint::black_box(handle);
                Ok::<(), anyhow::Error>(())
            };
            tokio::time::timeout(Duration::from_millis(1), work)
                .await
                .context("native pending work deadline")?
        })
        .await;
        ensure!(result.is_err());
        ensure!(
            closed.load(Ordering::SeqCst)
                && handle_dropped.load(Ordering::SeqCst)
                && retained.lock().await.is_none(),
            "registered cleanup did not close the retained slot"
        );
        // An interrupted close must never become success merely because it consumed its slot.
        let mut interrupted = Some(());
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        ensure!(
            tokio::time::timeout(
                Duration::from_millis(1),
                close_slot(&mut interrupted, |_| std::future::pending(), deadline)
            )
            .await
            .is_err()
        );
        ensure!(
            close_slot(&mut interrupted, |_| async { Ok(()) }, deadline)
                .await
                .is_err()
        );
        Ok(())
    }
}
