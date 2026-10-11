//! Future temporal events through the ordinary public caller and existing lifecycle.
use super::{cleanup, installed, open_receipt};
use anyhow::{Context, Result, ensure};
use rmcp::{model::*, service::Subscription};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Write,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Mutex as AsyncMutex;
use veoveo_gateway_contract::GatewayToolName;
use veoveo_testing_support::{
    SmokeMcpClient,
    installed::{
        restart::{DeploymentRestart, DrainProfile, DrainReceipt, SelectedDrainIdentity},
        tools,
    },
    lifecycle::owner,
};
use veoveo_time_mcp::{
    CancelTemporalEventRequest, ClockCurrent, CollectionPage, CreateTemporalEventRequest,
    EffectiveTimeAuthority, EventCursor, TemporalEvent, TemporalEventState, TimeInstant,
    TimeResource,
};
use veoveo_types::ResourceAddress;
#[cfg(test)]
#[path = "events/tests.rs"]
mod tests;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    installation: installed::InstalledSource,
    authority: EffectiveTimeAuthority,
    selected_pod: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Intent {
    request: CreateTemporalEventRequest,
    acknowledged: Option<TemporalEvent>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema: &'static str,
    phase: Phase,
    intents: Vec<Intent>,
    cancellation_intents: Vec<CancelTemporalEventRequest>,
    creation_replay_intents: Vec<CreateTemporalEventRequest>,
    observations: Vec<TemporalEvent>,
    delivered_invalidations: usize,
    selected: Option<SelectedDrainIdentity>,
    restart_intent: bool,
    restart: Option<DrainReceipt>,
    disconnected_due: Option<TemporalEvent>,
    listener_closed: bool,
    caller_closed: bool,
    failed: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Phase {
    Admitted,
    Armed,
    Created,
    LiveDue,
    Cancelled,
    RestartIntent,
    Reconnected,
    RestartDue,
    Disconnected,
    Reconciled,
    Passed,
    Failed,
}
struct Journal {
    file: fs::File,
    evidence: Evidence,
}
impl Journal {
    fn persist(&mut self) -> Result<()> {
        serde_json::to_writer(&mut self.file, &self.evidence)?;
        self.file.write_all(b"\n")?;
        self.file.sync_all()?;
        Ok(())
    }
    fn phase(&mut self, phase: Phase) -> Result<()> {
        self.evidence.phase = phase;
        self.persist()
    }
}
fn journal(j: &Arc<Mutex<Journal>>, update: impl FnOnce(&mut Journal) -> Result<()>) -> Result<()> {
    let mut guard = j
        .lock()
        .map_err(|_| anyhow::anyhow!("event journal lock poisoned"))?;
    update(&mut guard)
}
#[derive(Default)]
struct Generation {
    caller: Option<SmokeMcpClient>,
    listener: Option<Subscription>,
    caller_close: cleanup::CloseState,
    listener_close: cleanup::CloseState,
}
impl Generation {
    async fn close(&mut self, end: std::time::Instant) -> Result<()> {
        let listener = self
            .listener_close
            .close(
                &mut self.listener,
                |mut listener| async move {
                    listener
                        .cancel()
                        .await
                        .map_err(|_| anyhow::anyhow!("event listener close failed"))
                },
                end,
            )
            .await;
        let caller = self
            .caller_close
            .close(
                &mut self.caller,
                |caller| async move {
                    caller
                        .cancel()
                        .await
                        .map_err(|_| anyhow::anyhow!("event caller close failed"))
                },
                end,
            )
            .await;
        listener?;
        caller
    }
}
#[derive(Default)]
struct Handles {
    generations: Vec<Generation>,
}
fn due(clock: &ClockCurrent, seconds: i64) -> Result<TimeInstant> {
    ensure!(
        seconds > 0 && seconds <= 240,
        "event offset must be within 1..=240 seconds"
    );
    let now = clock.time.instant();
    TimeInstant::from_total_nanoseconds(
        now.total_nanoseconds()
            .checked_add(i128::from(seconds) * 1_000_000_000)
            .context("event due coordinate overflow")?,
        now.uncertainty_nanoseconds,
        now.authority.clone(),
    )
    .map_err(Into::into)
}
fn unchanged(
    created: &TemporalEvent,
    current: &TemporalEvent,
    state: TemporalEventState,
) -> Result<()> {
    ensure!(
        created.event_id == current.event_id
            && created.name == current.name
            && created.due == current.due
            && current.state == state
            && current.record_version == created.record_version.checked_next()?,
        "event transition identity, payload, state or version differs"
    );
    Ok(())
}
async fn read<T: DeserializeOwned>(
    peer: &rmcp::Peer<rmcp::RoleClient>,
    resource: TimeResource,
) -> Result<T> {
    // Bypass SDK resource caching: each observation is a new authoritative request.
    let uri = resource.to_uri()?;
    let request = ClientRequest::ReadResourceRequest(ReadResourceRequest::new(
        ReadResourceRequestParams::new(uri.as_str()),
    ));
    let ServerResult::ReadResourceResult(result) = peer.send_request(request).await? else {
        anyhow::bail!("event read response kind differs")
    };
    let [
        ResourceContents::TextResourceContents {
            uri: actual, text, ..
        },
    ] = result.contents.as_slice()
    else {
        anyhow::bail!("event read requires one JSON resource")
    };
    ensure!(
        actual == uri.as_str() && text.len() <= 64 * 1024,
        "event read resource identity/size differs"
    );
    Ok(serde_json::from_str(text)?)
}
fn tool(operation: &str) -> Result<GatewayToolName> {
    Ok(GatewayToolName::from_parts(
        &"time".parse()?,
        &operation.parse()?,
    )?)
}
async fn refused<I: Serialize>(
    peer: &rmcp::Peer<rmcp::RoleClient>,
    operation: &str,
    request: &I,
) -> Result<()> {
    let arguments = serde_json::to_value(request)?
        .as_object()
        .cloned()
        .context("event request must be object")?;
    let result = peer
        .call_tool(
            CallToolRequestParams::new(tool(operation)?.to_string()).with_arguments(arguments),
        )
        .await;
    // A dropped transport or expired wait cannot qualify domain conflict refusal.
    ensure!(
        matches!(result,Err(rmcp::ServiceError::McpError(ref error)) if error.code==ErrorCode::INTERNAL_ERROR && error.message.contains("conflict")),
        "event guard/idempotency refusal did not return the documented conflict"
    );
    Ok(())
}
async fn arm(input: &Input, handles: &mut Handles, j: &Arc<Mutex<Journal>>) -> Result<()> {
    handles.generations.push(Generation::default());
    let generation = handles.generations.last_mut().unwrap();
    generation.caller = Some(input.installation.task_caller().await?);
    let caller = generation.caller.as_ref().unwrap();
    let authority: EffectiveTimeAuthority =
        read(caller.peer(), TimeResource::AuthoritiesCurrent).await?;
    ensure!(
        authority == input.authority,
        "event reconnect authority changed"
    );
    let uri = TimeResource::Events { cursor: None }.to_uri()?;
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions([uri.to_string()])
        .build();
    generation.listener = Some(caller.listen(filter.clone()).await?);
    ensure!(
        generation.listener.as_ref().unwrap().acknowledged() == &filter,
        "event root exact filter not acknowledged"
    );
    let _: CollectionPage<TemporalEvent, EventCursor> =
        read(caller.peer(), TimeResource::Events { cursor: None }).await?;
    journal(j, |j| j.phase(Phase::Armed))
}
async fn delivered_due(
    generation: &mut Generation,
    created: &TemporalEvent,
    j: &Arc<Mutex<Journal>>,
) -> Result<()> {
    loop {
        let listener = generation
            .listener
            .as_mut()
            .context("event listener absent")?;
        let notification = listener
            .next()
            .await?
            .context("event subscription ended before Due")?;
        ensure!(
            notification.get_meta().subscription_id().as_ref() == Some(listener.id()),
            "event notification subscription differs"
        );
        let ServerNotification::ResourceUpdatedNotification(update) = notification else {
            anyhow::bail!("event listener notification kind differs")
        };
        ensure!(
            TimeResource::parse(&update.params.uri)? == (TimeResource::Events { cursor: None }),
            "event listener resource differs"
        );
        journal(j, |j| {
            j.evidence.delivered_invalidations += 1;
            j.persist()
        })?;
        let current: TemporalEvent = read(
            generation.caller.as_ref().unwrap().peer(),
            TimeResource::Event(created.event_id.clone()),
        )
        .await?;
        if current.state == TemporalEventState::Due {
            unchanged(created, &current, TemporalEventState::Due)?;
            return journal(j, |j| {
                j.evidence.observations.push(current);
                j.persist()
            });
        }
        ensure!(current == *created, "event changed before Due");
    }
}
async fn exercise(
    input: &Input,
    target: &veoveo_deploy_contract::InstallationTarget,
    handles: &mut Handles,
    j: &Arc<Mutex<Journal>>,
    end: tokio::time::Instant,
) -> Result<()> {
    arm(input, handles, j).await?;
    let caller = handles.generations.last().unwrap().caller.as_ref().unwrap();
    let clock: ClockCurrent = read(caller.peer(), TimeResource::ClockCurrent).await?;
    let clock_anchor = tokio::time::Instant::now();
    ensure!(
        clock.time.instant().authority == input.authority.binding(),
        "event current authority differs"
    );
    let mut created = Vec::new();
    for (index, seconds) in [20, 80, 180, 240].into_iter().enumerate() {
        let request = CreateTemporalEventRequest {
            name: format!("Temporal acceptance {index}"),
            due: due(&clock, seconds)?,
            idempotency_key: uuid::Uuid::now_v7().to_string(),
        };
        journal(j, |j| {
            j.evidence.intents.push(Intent {
                request: request.clone(),
                acknowledged: None,
            });
            j.persist()
        })?;
        let event: TemporalEvent =
            tools::call(caller.peer(), tool("create_temporal_event")?, &request).await?;
        // Retain a decoded acknowledgement synchronously before another await.
        journal(j, |j| {
            j.evidence.intents[index].acknowledged = Some(event.clone());
            j.persist()
        })?;
        ensure!(
            event.name == request.name
                && event.due == request.due
                && event.state == TemporalEventState::Scheduled
                && event.record_version.get() == 1,
            "event create receipt differs"
        );
        journal(j, |j| {
            j.evidence.creation_replay_intents.push(request.clone());
            j.persist()
        })?;
        let replay: TemporalEvent =
            tools::call(caller.peer(), tool("create_temporal_event")?, &request).await?;
        ensure!(replay == event, "event idempotent replay differs");
        let mut conflict = request;
        conflict.name.push_str(" conflict");
        journal(j, |j| {
            j.evidence.creation_replay_intents.push(conflict.clone());
            j.persist()
        })?;
        refused(caller.peer(), "create_temporal_event", &conflict).await?;
        let current: TemporalEvent =
            read(caller.peer(), TimeResource::Event(event.event_id.clone())).await?;
        ensure!(
            current == event,
            "idempotency conflict changed original event"
        );
        created.push(event);
    }
    journal(j, |j| j.phase(Phase::Created))?;
    delivered_due(handles.generations.last_mut().unwrap(), &created[0], j).await?;
    journal(j, |j| j.phase(Phase::LiveDue))?;
    let caller = handles.generations.last().unwrap().caller.as_ref().unwrap();
    let request = CancelTemporalEventRequest {
        event_id: created[1].event_id.clone(),
        expected_record_version: created[1].record_version,
    };
    journal(j, |j| {
        j.evidence.cancellation_intents.push(request.clone());
        j.persist()
    })?;
    let cancelled: TemporalEvent =
        tools::call(caller.peer(), tool("cancel_temporal_event")?, &request).await?;
    journal(j, |j| {
        j.evidence.observations.push(cancelled.clone());
        j.persist()
    })?;
    unchanged(&created[1], &cancelled, TemporalEventState::Cancelled)?;
    journal(j, |j| {
        j.evidence.cancellation_intents.push(request.clone());
        j.persist()
    })?;
    refused(caller.peer(), "cancel_temporal_event", &request).await?;
    let current: TemporalEvent = read(
        caller.peer(),
        TimeResource::Event(cancelled.event_id.clone()),
    )
    .await?;
    ensure!(current == cancelled, "stale cancellation mutated event");
    journal(j, |j| j.phase(Phase::Cancelled))?;
    let restart = DeploymentRestart::new(
        target,
        &input.installation.deployment,
        "time-mcp",
        caller.peer().clone(),
        veoveo_mcp_contract::ServerResourceUris::new("time".parse()?).contract_uri(),
    )?;
    let selected = restart
        .select_drain_target(
            &input.selected_pod,
            DrainProfile::server("time-mcp", Duration::from_secs(30))?,
        )
        .await?;
    let before: TemporalEvent = read(
        caller.peer(),
        TimeResource::Event(created[2].event_id.clone()),
    )
    .await?;
    ensure!(before == created[2], "future restart event already changed");
    journal(j, |j| {
        j.evidence.selected = Some(selected.identity());
        j.evidence.restart_intent = true;
        j.phase(Phase::RestartIntent)
    })?;
    let receipt = restart.restart_with_drain(&selected).await?;
    journal(j, |j| {
        j.evidence.restart = Some(receipt);
        j.persist()
    })?;
    handles
        .generations
        .last_mut()
        .unwrap()
        .close(end.into_std())
        .await?;
    arm(input, handles, j).await?;
    let caller = handles.generations.last().unwrap().caller.as_ref().unwrap();
    let retained: TemporalEvent = read(
        caller.peer(),
        TimeResource::Event(created[2].event_id.clone()),
    )
    .await?;
    ensure!(
        retained == created[2],
        "replacement future event must still be Scheduled"
    );
    journal(j, |j| j.phase(Phase::Reconnected))?;
    delivered_due(handles.generations.last_mut().unwrap(), &created[2], j).await?;
    journal(j, |j| j.phase(Phase::RestartDue))?;
    // Deliberately miss one notification. Reconnection qualifies current-state
    // reconciliation, never notification replay or causal notification revision.
    handles
        .generations
        .last_mut()
        .unwrap()
        .close(end.into_std())
        .await?;
    journal(j, |j| j.phase(Phase::Disconnected))?;
    tokio::time::sleep_until(clock_anchor + Duration::from_secs(242)).await;
    arm(input, handles, j).await?;
    let caller = handles.generations.last().unwrap().caller.as_ref().unwrap();
    let missed: TemporalEvent = read(
        caller.peer(),
        TimeResource::Event(created[3].event_id.clone()),
    )
    .await?;
    unchanged(&created[3], &missed, TemporalEventState::Due)?;
    let cancelled_current: TemporalEvent = read(
        caller.peer(),
        TimeResource::Event(created[1].event_id.clone()),
    )
    .await?;
    ensure!(
        cancelled_current == cancelled,
        "cancelled event changed after its due instant"
    );
    journal(j, |j| {
        j.evidence.disconnected_due = Some(missed);
        j.phase(Phase::Reconciled)
    })
}
fn cleanup_cancellation(
    acknowledged: &TemporalEvent,
    current: &TemporalEvent,
) -> Result<Option<CancelTemporalEventRequest>> {
    ensure!(
        current.event_id == acknowledged.event_id
            && current.name == acknowledged.name
            && current.due == acknowledged.due,
        "cleanup event identity or ownership payload differs"
    );
    Ok(
        (current.state == TemporalEventState::Scheduled).then(|| CancelTemporalEventRequest {
            event_id: acknowledged.event_id.clone(),
            expected_record_version: current.record_version,
        }),
    )
}
async fn cleanup(handles: &mut Handles, j: &Arc<Mutex<Journal>>) -> Result<()> {
    let end = owner::cleanup_deadline()?;
    let snapshot = j
        .lock()
        .map(|j| j.evidence.intents.clone())
        .map_err(|_| anyhow::anyhow!("event journal lock poisoned"));
    let (intents, mut failure) = match snapshot {
        Ok(intents) => (intents, None),
        Err(error) => (vec![], Some(error)),
    };
    let has_ack = intents.iter().any(|intent| intent.acknowledged.is_some());
    if intents.iter().any(|intent| intent.acknowledged.is_none()) {
        failure.get_or_insert_with(|| {
            anyhow::anyhow!(
                "event creation outcome remains unknown; retain intent for operator reconciliation"
            )
        });
    }
    let peer = handles
        .generations
        .iter()
        .rev()
        .find_map(|g| g.caller.as_ref().map(|caller| caller.peer().clone()));
    if let Some(peer) = peer {
        let reconciled = tokio::time::timeout_at((end - Duration::from_secs(5)).into(), async {
            for intent in intents {
                if let Some(event) = intent.acknowledged {
                    let current: TemporalEvent =
                        read(&peer, TimeResource::Event(event.event_id.clone())).await?;
                    if let Some(request) = cleanup_cancellation(&event, &current)? {
                        journal(j, |j| {
                            j.evidence.cancellation_intents.push(request.clone());
                            j.persist()
                        })?;
                        let cancelled: TemporalEvent =
                            tools::call(&peer, tool("cancel_temporal_event")?, &request).await?;
                        unchanged(&current, &cancelled, TemporalEventState::Cancelled)?;
                        journal(j, |j| {
                            j.evidence.observations.push(cancelled);
                            j.persist()
                        })?;
                    }
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("event reconciliation cleanup deadline")
        .and_then(|r| r);
        if let Err(error) = reconciled {
            failure = Some(error);
        }
    } else if has_ack {
        failure.get_or_insert_with(|| {
            anyhow::anyhow!(
                "event cleanup lacks a live same-owner caller; retained events unresolved"
            )
        });
    }
    for generation in &mut handles.generations {
        if let Err(error) = generation.close(end).await {
            failure.get_or_insert(error);
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
#[tokio::test]
#[ignore = "requires same-owner OAuth event-write/read, selected immutable authority and one admitted disposable Time process restart"]
async fn future_temporal_events_through_public_gateway() -> Result<()> {
    let path = std::path::PathBuf::from(
        std::env::var_os("VEOVEO_TIME_EVENTS_INPUT").context("set private Time event fixture")?,
    );
    let input: Input =
        veoveo_testing_support::final_tasks::public_caller::read_private_input(&path)?;
    let target = input.installation.validate()?;
    ensure!(
        input.installation.deployment == "time-mcp",
        "event fixture must select Time"
    );
    let end = tokio::time::Instant::now() + Duration::from_secs(300);
    let j = Arc::new(Mutex::new(Journal {
        file: open_receipt(&input.installation.output)?,
        evidence: Evidence {
            schema: "veoveo.ai/time-event-acceptance/v1",
            phase: Phase::Admitted,
            intents: vec![],
            cancellation_intents: vec![],
            creation_replay_intents: vec![],
            observations: vec![],
            delivered_invalidations: 0,
            selected: None,
            restart_intent: false,
            restart: None,
            disconnected_due: None,
            listener_closed: true,
            caller_closed: true,
            failed: false,
        },
    }));
    journal(&j, |j| j.persist())?;
    let handles = Arc::new(AsyncMutex::new(Handles::default()));
    let result = owner::run(async {
        let captured = handles.clone();
        let retained = j.clone();
        owner::register_cleanup(
            owner::CleanupKind::Remote,
            "Time future events and SDK handles",
            &uuid::Uuid::now_v7().to_string(),
            move || async move { cleanup(&mut *captured.lock().await, &retained).await },
        )?;
        tokio::time::timeout_at(
            end,
            exercise(&input, &target, &mut *handles.lock().await, &j, end),
        )
        .await
        .context("original Time event deadline elapsed")?
    })
    .await;
    let handles = handles.lock().await;
    journal(&j, |j| {
        j.evidence.listener_closed = handles
            .generations
            .iter()
            .all(|g| g.listener.is_none() && g.listener_close.closed());
        j.evidence.caller_closed = handles
            .generations
            .iter()
            .all(|g| g.caller.is_none() && g.caller_close.closed());
        j.evidence.failed = result.is_err();
        j.phase(if result.is_ok() {
            Phase::Passed
        } else {
            Phase::Failed
        })
    })?;
    ensure!(
        result.is_ok(),
        "Time future-event qualification failed; inspect private journal and ownership lease"
    );
    Ok(())
}
