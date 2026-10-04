//! Personal observation shares projected database hints and exact native Tasks.
//! Reconnect reads current state; no observation path submits or resumes work.
use super::*;
use axum::response::{
    Sse,
    sse::{Event, KeepAlive},
};
use futures::StreamExt;
use rmcp::{
    model::{ServerNotification, SubscriptionFilter},
    service::Subscription,
};
use std::{collections::BTreeMap, convert::Infallible};
use tokio::sync::broadcast;
use veoveo_platform_store::{RecordId, workspace::WorkspacePersonalState};

const MAX_TASKS: usize = 32;

#[derive(Clone)]
pub(super) struct PersonalHub {
    wakes: broadcast::Sender<Hint>,
}
#[derive(Clone)]
enum Hint {
    Principal(RecordId),
    Reconcile,
}
impl PersonalHub {
    pub fn new(store: PlatformStore, stop: CancellationToken) -> Self {
        let (wakes, _) = broadcast::channel(256);
        let publish = wakes.clone();
        tokio::spawn(async move {
            loop {
                let source = tokio::select! { _ = stop.cancelled() => return, source = store.workspace_personal_wakes() => source };
                if let Ok(mut source) = source {
                    let _ = publish.send(Hint::Reconcile);
                    loop {
                        tokio::select! {
                            _ = stop.cancelled() => return,
                            event = source.next() => match event {
                                Some(Ok(event)) => { let _ = publish.send(Hint::Principal(event.principal)); }
                                _ => break,
                            }
                        }
                    }
                }
                tokio::select! { _ = stop.cancelled() => return, _ = tokio::time::sleep(Duration::from_secs(1)) => {} }
            }
        });
        Self { wakes }
    }
}

pub(super) async fn events(
    State(state): State<OperationState>,
    Path(raw_profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let caller = Caller::new(profile(raw_profile)?, subject, &headers)?;
    let slot = state
        .watches
        .acquire(caller.subject.principal.id.as_str())
        .ok_or(StatusCode::TOO_MANY_REQUESTS)?;
    let catalog = state.catalog.current();
    let mut wakes = state.personal.wakes.subscribe();
    let actor = state.authority(&caller.subject, &caller.profile).await?;
    let mut snapshot = state
        .workspace
        .store
        .workspace_personal_state(&actor, caller.profile.as_str())
        .await
        .map_err(fault)?;
    if caller.subject.access_token.expires_at <= chrono::Utc::now() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let first = inventory(&snapshot)?;
    let selected = task_operations(&snapshot)?;
    let deadline = tokio::time::Instant::now()
        + (caller.subject.access_token.expires_at - chrono::Utc::now())
            .to_std()
            .unwrap_or_default()
            .min(Duration::from_secs(55));
    let stream = async_stream::stream! {
        let _slot = slot;
        yield Ok(event(first));
        let mut native = tokio::select! {
            _ = state.stop.cancelled() => return,
            _ = tokio::time::sleep_until(deadline) => return,
            result = open_tasks(&state, &caller, &selected) => result.ok().flatten(),
        };
        if !Arc::ptr_eq(&catalog, &state.catalog.current()) || state.authority(&caller.subject, &caller.profile).await.is_err() { return; }
        if let Some(tasks) = &mut native {
            for initial in tasks.initial.drain(..) { yield Ok(event(initial)); }
        }
        yield Ok(event(wire::PersonalEvent::Availability { live_tasks: selected.is_empty() || native.is_some() }));
        let mut reconcile = tokio::time::interval(Duration::from_secs(15));
        reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        reconcile.tick().await;
        loop {
            let notification = tokio::select! {
                _ = state.stop.cancelled() => break,
                _ = tokio::time::sleep_until(deadline) => break,
                _ = reconcile.tick() => None,
                hint = wakes.recv() => match hint {
                    Ok(Hint::Principal(principal)) if principal != actor.principal => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                    _ => None,
                },
                update = next_task(&mut native) => match update {
                    Ok(Some(update)) => Some(update),
                    _ => break, // Browser reconnect replaces ended native observation.
                }
            };
            if !Arc::ptr_eq(&catalog, &state.catalog.current()) { break; }
            let Ok(authority) = state.authority(&caller.subject, &caller.profile).await else {
                yield Ok(Event::default().event("expired").data("{}")); break;
            };
            let Ok(Ok(current)) = tokio::time::timeout(Duration::from_secs(5), state.workspace.store.workspace_personal_state(&authority, caller.profile.as_str())).await else {
                yield Ok(Event::default().event("expired").data("{}")); break;
            };
            if tokio::time::Instant::now() >= deadline { break; }
            if current != snapshot {
                let Ok(value) = inventory(&current) else { break; };
                yield Ok(event(value));
                let Ok(tasks) = task_operations(&current) else { break; };
                if tasks != selected { break; } // Renew the exact filter after durable admission.
                snapshot = current;
            }
            if let Some(ServerNotification::TaskStatusNotification(update)) = notification {
                let task = update.params.task.task;
                if let Some(operation) = selected.get(&task.task_id) {
                    let Ok(task_state) = projection::task_state(task.status) else { break; };
                    yield Ok(event(wire::PersonalEvent::Task { operation: *operation, state: task_state, updated_at: task.last_updated_at }));
                }
            }
            // Lost connection establishment retries through fresh HTTP admission.
            if !selected.is_empty() && native.is_none() { break; }
        }
        if let Some(mut tasks) = native {
            if let Some(mut listener) = tasks.listener.take() {
                let _ = tokio::time::timeout(Duration::from_secs(2), listener.cancel()).await;
            }
            tasks.client.close().await;
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(10))))
}

fn event(value: wire::PersonalEvent) -> Event {
    Event::default()
        .event("personal")
        .retry(Duration::from_secs(2))
        .json_data(value)
        .expect("typed personal event")
}

fn inventory(value: &WorkspacePersonalState) -> Result<wire::PersonalEvent, StatusCode> {
    use super::super::projection::uuid;
    Ok(wire::PersonalEvent::Inventory {
        operations: value
            .operations
            .iter()
            .map(|operation| {
                Ok(wire::PersonalOperation {
                    id: wire::OperationId(uuid(&operation.id)?),
                    revision: operation.revision,
                    phase: match operation.phase {
                        Phase::Dispatching => wire::OperationPhase::Dispatching,
                        Phase::InputRequired => wire::OperationPhase::InputRequired,
                        Phase::Task => wire::OperationPhase::Task,
                        Phase::Completed => wire::OperationPhase::Completed,
                        Phase::Failed => wire::OperationPhase::Failed,
                        Phase::Unconfirmed => wire::OperationPhase::Unconfirmed,
                    },
                })
            })
            .collect::<Result<_, StatusCode>>()?,
        invitations: value.invitations.len() as u16,
        limited: value
            .operations
            .iter()
            .filter(|operation| operation.task_id.is_some())
            .count()
            > MAX_TASKS
            || value.operations.len() == 100,
    })
}

fn task_operations(
    value: &WorkspacePersonalState,
) -> Result<BTreeMap<String, wire::OperationId>, StatusCode> {
    value
        .operations
        .iter()
        .filter_map(|operation| operation.task_id.as_ref().map(|task| (task, &operation.id)))
        .take(MAX_TASKS)
        .map(|(task, id)| {
            Ok((
                task.clone(),
                wire::OperationId(super::super::projection::uuid(id)?),
            ))
        })
        .collect()
}

struct NativeTasks {
    client: native::NativeClient,
    listener: Option<Subscription>,
    initial: Vec<wire::PersonalEvent>,
}
async fn open_tasks(
    state: &OperationState,
    caller: &Caller,
    tasks: &BTreeMap<String, wire::OperationId>,
) -> Result<Option<NativeTasks>, StatusCode> {
    if tasks.is_empty() {
        return Ok(None);
    }
    let client = state
        .native
        .connect(&caller.profile, &caller.bearer)
        .await?;
    // Reconcile with current native authority before listening. Retained receipts
    // can outlive Task retention; one unavailable Task must not disable others.
    let peer = client.peer().clone();
    let mut reads = futures::stream::iter(tasks.clone().into_iter().map(move |(id, operation)| {
        let peer = peer.clone();
        async move {
            let result = tokio::time::timeout(
                Duration::from_secs(8),
                peer.get_task(GetTaskParams::new(id.clone())),
            )
            .await;
            (id, operation, result)
        }
    }))
    .buffer_unordered(4);
    let mut initial = vec![];
    let mut active = vec![];
    while let Some((id, operation, result)) = reads.next().await {
        if let Ok(Ok(result)) = result
            && let Ok(status) = projection::task_state(result.task.task.status)
        {
            if matches!(
                status,
                wire::TaskState::Working | wire::TaskState::InputRequired
            ) {
                active.push(id);
            }
            initial.push(wire::PersonalEvent::Task {
                operation,
                state: status,
                updated_at: result.task.task.last_updated_at,
            });
        } else {
            initial.push(wire::PersonalEvent::TaskUnavailable { operation });
        }
    }
    drop(reads);
    let listener = if active.is_empty() {
        None
    } else {
        let filter = SubscriptionFilter::builder().task_ids(active).build();
        let listener =
            tokio::time::timeout(Duration::from_secs(8), client.peer().listen(filter.clone()))
                .await
                .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
                .map_err(mcp_error)?;
        if listener.acknowledged() != &filter {
            return Err(StatusCode::FORBIDDEN);
        }
        Some(listener)
    };
    Ok(Some(NativeTasks {
        client,
        listener,
        initial,
    }))
}
async fn next_task(
    native: &mut Option<NativeTasks>,
) -> Result<Option<ServerNotification>, rmcp::ServiceError> {
    match native {
        Some(NativeTasks {
            listener: Some(listener),
            ..
        }) => listener.next().await,
        _ => std::future::pending().await,
    }
}
