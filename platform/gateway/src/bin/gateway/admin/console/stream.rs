use std::{
    collections::BTreeMap,
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{Extension, Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use chrono::{DateTime, Utc};
use futures::Stream;
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::GatewayAction;
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{
    AgentRecord, ArtifactBlobRecord, ArtifactGrantEdge, ArtifactOccurrenceRecord, AuditEventRecord,
    ChangefeedCursor, ChangefeedEntry, PlatformStore, PlatformTable, PrincipalRecord, RecordId,
    RecordingRecord, SegmentRecord, ShareLinkRecord, TaskRecord, Value as DbValue, WakeRecord,
    decode_changefeed_entry, deterministic_tenant_id,
};

use super::projection::{
    ArtifactGrantSummary, ArtifactShareLinkSummary, agent_summary, artifact_grant_summary,
    artifact_summary, audit_summary, load_projection, record_key, recording_summary,
    server_summary, share_link_summary, task_summary,
};
use crate::{
    admin::admin_profile_id,
    audit::authorize_admin_request,
    runtime::{AdminState, current_catalog},
};

const MAX_CONCURRENT_STREAMS: usize = 16;
const MAX_STREAMS_PER_PRINCIPAL: usize = 3;
const MAX_STREAM_LIFETIME: Duration = Duration::from_secs(15 * 60);
const RECONCILE_INTERVAL: Duration = Duration::from_secs(15);
const WAKE_DEBOUNCE: Duration = Duration::from_millis(200);
const LIVE_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const REPLAY_PAGE_LIMIT: u32 = 1_000;
const RETRY_HINT_MS: u32 = 3_000;
/// Cursors older than this force a full resync instead of replaying a huge
/// backlog; a browser away that long should refetch the snapshot anyway.
const REPLAY_HORIZON: chrono::TimeDelta = chrono::TimeDelta::hours(24);

/// Tenant tables the console stream follows, in dependency order: within one
/// versionstamp group parents apply before the children that re-emit them.
const STREAM_TABLES: [PlatformTable; 11] = [
    PlatformTable::Principal,
    PlatformTable::Task,
    PlatformTable::ArtifactBlob,
    PlatformTable::ArtifactOccurrence,
    PlatformTable::ArtifactGrant,
    PlatformTable::ShareLink,
    PlatformTable::Agent,
    PlatformTable::Wake,
    PlatformTable::Recording,
    PlatformTable::Segment,
    PlatformTable::AuditEvent,
];

const fn table_rank(table: PlatformTable) -> usize {
    let mut index = 0;
    while index < STREAM_TABLES.len() {
        if STREAM_TABLES[index] as usize == table as usize {
            return index;
        }
        index += 1;
    }
    usize::MAX
}

/// Shared console-stream runtime: one process-wide LIVE wake hub plus
/// connection limits. LIVE notifications are contentless wake signals only;
/// every event a client sees comes from durable changefeed replay.
#[derive(Clone)]
pub(crate) struct ConsoleStreamRuntime {
    wake: watch::Receiver<u64>,
    limits: Arc<StreamLimits>,
}

struct StreamLimits {
    global: Arc<Semaphore>,
    per_principal: Mutex<BTreeMap<String, usize>>,
}

struct StreamSlot {
    _global: OwnedSemaphorePermit,
    limits: Arc<StreamLimits>,
    principal: String,
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        let mut per_principal = self.limits.per_principal.lock();
        if let Some(count) = per_principal.get_mut(&self.principal) {
            *count -= 1;
            if *count == 0 {
                per_principal.remove(&self.principal);
            }
        }
    }
}

impl ConsoleStreamRuntime {
    fn acquire(&self, principal: &str) -> Option<StreamSlot> {
        let global = self.global_permit()?;
        let mut per_principal = self.limits.per_principal.lock();
        let count = per_principal.entry(principal.to_owned()).or_default();
        if *count >= MAX_STREAMS_PER_PRINCIPAL {
            return None;
        }
        *count += 1;
        Some(StreamSlot {
            _global: global,
            limits: self.limits.clone(),
            principal: principal.to_owned(),
        })
    }

    fn global_permit(&self) -> Option<OwnedSemaphorePermit> {
        self.limits.global.clone().try_acquire_owned().ok()
    }
}

pub(crate) fn spawn_console_wake_hub(
    store: PlatformStore,
    cancellation: CancellationToken,
) -> ConsoleStreamRuntime {
    let (wake_tx, wake_rx) = watch::channel(0u64);
    let wake_tx = Arc::new(wake_tx);
    for table in STREAM_TABLES {
        let store = store.clone();
        let wake_tx = wake_tx.clone();
        let cancellation = cancellation.clone();
        tokio::spawn(async move {
            table_wake_loop(store, table, wake_tx, cancellation).await;
        });
    }
    ConsoleStreamRuntime {
        wake: wake_rx,
        limits: Arc::new(StreamLimits {
            global: Arc::new(Semaphore::new(MAX_CONCURRENT_STREAMS)),
            per_principal: Mutex::new(BTreeMap::new()),
        }),
    }
}

/// One reconnecting LIVE subscription per table, exactly like the
/// artifact-mcp outbox wake loop: LIVE items are dropped on the floor, they
/// only bump the wake epoch. A LIVE failure loses nothing — replay covers
/// the gap after reconnect.
async fn table_wake_loop(
    store: PlatformStore,
    table: PlatformTable,
    wake: Arc<watch::Sender<u64>>,
    cancellation: CancellationToken,
) {
    use futures::StreamExt;
    loop {
        let mut live = match store.live::<DbValue>(table).await {
            Ok(live) => live,
            Err(error) => {
                tracing::warn!(%table, "console wake LIVE connect failed: {error}");
                tokio::select! {
                    () = cancellation.cancelled() => return,
                    () = tokio::time::sleep(LIVE_RECONNECT_DELAY) => continue,
                }
            }
        };
        loop {
            tokio::select! {
                () = cancellation.cancelled() => return,
                item = live.next() => match item {
                    Some(Ok(_)) => {
                        wake.send_modify(|epoch| *epoch += 1);
                    }
                    Some(Err(error)) => {
                        tracing::warn!(%table, "console wake LIVE stream failed: {error}");
                        break;
                    }
                    None => break,
                },
            }
        }
        tokio::select! {
            () = cancellation.cancelled() => return,
            () = tokio::time::sleep(LIVE_RECONNECT_DELAY) => {}
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct StreamQuery {
    cursor: Option<String>,
}

pub(crate) async fn stream_console(
    State(state): State<AdminState>,
    AxumPath(profile): AxumPath<String>,
    Query(query): Query<StreamQuery>,
    headers: HeaderMap,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (_catalog, _profile, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminRead,
        "admin/console/stream",
        BTreeMap::new(),
        started_at,
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let tenant_key = subject
        .principal
        .tenant
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "installation".to_owned());
    let tenant = match deterministic_tenant_id(&tenant_key) {
        Ok(tenant) => tenant.record_id(),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(slot) = state.console_stream.acquire(subject.principal.id.as_ref()) else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };

    // Last-Event-ID (reconnect) takes precedence over the snapshot cursor.
    let requested_cursor = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
        .or(query.cursor);
    let store = state.control_store.platform_store().clone();
    let cursor = match resolve_cursor(&store, requested_cursor.as_deref()).await {
        Ok(cursor) => cursor,
        Err(response) => return response,
    };

    let deadline = stream_deadline(subject.access_token.expires_at);
    let stream = console_event_stream(state, store, tenant, cursor, slot, deadline);
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(10))
                .text("keep-alive"),
        )
        .into_response()
}

async fn resolve_cursor(
    store: &PlatformStore,
    requested: Option<&str>,
) -> Result<ChangefeedCursor, Response> {
    match requested {
        Some(raw) => {
            let cursor = raw
                .parse::<i64>()
                .ok()
                .and_then(ChangefeedCursor::from_versionstamp);
            let Some(cursor) = cursor else {
                return Err(StatusCode::BAD_REQUEST.into_response());
            };
            // A cursor beyond the replay horizon forces a resync: the reset
            // event tells the client to refetch the snapshot rather than
            // replay an unbounded backlog.
            let implied_ms = cursor.versionstamp() >> 16;
            let horizon = Utc::now() - REPLAY_HORIZON;
            if cursor.versionstamp() > 0 && implied_ms < horizon.timestamp_millis() {
                return Err(reset_response("cursor-out-of-range"));
            }
            Ok(cursor)
        }
        None => store.changefeed_cursor_now().await.map_err(|error| {
            tracing::error!("console stream cursor anchor failed: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }),
    }
}

fn reset_response(reason: &str) -> Response {
    let body =
        format!("retry: {RETRY_HINT_MS}\nevent: reset\ndata: {{\"reason\":\"{reason}\"}}\n\n");
    (
        StatusCode::OK,
        [
            ("content-type", "text/event-stream"),
            ("cache-control", "no-store"),
        ],
        body,
    )
        .into_response()
}

fn stream_deadline(token_expires_at: DateTime<Utc>) -> tokio::time::Instant {
    let by_token = (token_expires_at - Utc::now())
        .to_std()
        .unwrap_or(Duration::ZERO);
    tokio::time::Instant::now() + by_token.min(MAX_STREAM_LIFETIME)
}

struct OutEvent {
    versionstamp: i64,
    rank: usize,
    name: &'static str,
    payload: serde_json::Value,
}

fn console_event_stream(
    state: AdminState,
    store: PlatformStore,
    tenant: RecordId,
    cursor: ChangefeedCursor,
    slot: StreamSlot,
    deadline: tokio::time::Instant,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        // Owned by the generator so the limit slot lives exactly as long as
        // the response stream.
        let _slot = slot;
        let mut wake = state.console_stream.wake.clone();
        let mut health_epoch = state.server_health.epoch.clone();
        wake.mark_changed();

        yield Ok(Event::default().retry(Duration::from_millis(RETRY_HINT_MS as u64)));

        let mut projection_state = match ConsoleStreamState::seed(&state, &tenant, cursor).await {
            Ok(seeded) => seeded,
            Err(error) => {
                tracing::error!("console stream seed failed: {error}");
                yield Ok(reset_event("seed-failed"));
                return;
            }
        };

        // Initial server-health frames: health has no changefeed cursor, so
        // the full set is emitted at connect and on every state change.
        for event in server_health_events(&state) {
            yield Ok(event);
        }

        let mut reconcile = tokio::time::interval(RECONCILE_INTERVAL);
        reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        reconcile.reset();

        loop {
            tokio::select! {
                () = tokio::time::sleep_until(deadline) => {
                    // Clean end-of-stream: the browser reconnects with
                    // Last-Event-ID and the BFF run refreshes the token.
                    return;
                }
                changed = health_epoch.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    for event in server_health_events(&state) {
                        yield Ok(event);
                    }
                }
                changed = wake.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    // Debounce so a burst of writes coalesces into one replay.
                    tokio::time::sleep(WAKE_DEBOUNCE).await;
                    wake.mark_unchanged();
                    match projection_state.drain(&store).await {
                        Ok(events) => {
                            for event in group_events(events) {
                                yield Ok(event);
                            }
                        }
                        Err(error) => {
                            tracing::warn!("console stream replay failed: {error}");
                            yield Ok(reset_event("replay-failed"));
                            return;
                        }
                    }
                }
                _ = reconcile.tick() => {
                    match projection_state.drain(&store).await {
                        Ok(events) => {
                            for event in group_events(events) {
                                yield Ok(event);
                            }
                        }
                        Err(error) => {
                            tracing::warn!("console stream reconcile failed: {error}");
                            yield Ok(reset_event("replay-failed"));
                            return;
                        }
                    }
                }
            }
        }
    }
}

fn reset_event(reason: &str) -> Event {
    Event::default()
        .event("reset")
        .data(serde_json::json!({ "reason": reason }).to_string())
}

fn server_health_events(state: &AdminState) -> Vec<Event> {
    let catalog = current_catalog(&state.catalog);
    let control = catalog.control_plane();
    let health = state.server_health.snapshot();
    let now = Utc::now();
    control
        .servers
        .iter()
        .map(|server| {
            let summary = server_summary(server, control, health.get(&server.slug), now);
            Event::default()
                .event("server")
                .data(serde_json::json!({ "op": "upsert", "row": summary }).to_string())
        })
        .collect()
}

/// Sorts a replay round by (versionstamp, dependency rank) and attaches the
/// SSE `id:` to the last event of each versionstamp group, so an interrupted
/// group is replayed whole on reconnect.
fn group_events(mut events: Vec<OutEvent>) -> Vec<Event> {
    events.sort_by_key(|event| (event.versionstamp, event.rank));
    let mut rendered = Vec::with_capacity(events.len());
    let mut iter = events.into_iter().peekable();
    while let Some(event) = iter.next() {
        let is_group_boundary = iter
            .peek()
            .is_none_or(|next| next.versionstamp != event.versionstamp);
        let mut sse = Event::default()
            .event(event.name)
            .data(event.payload.to_string());
        if is_group_boundary {
            sse = sse.id(event.versionstamp.to_string());
        }
        rendered.push(sse);
    }
    rendered
}

struct ConsoleStreamState {
    tenant: RecordId,
    cursors: BTreeMap<usize, ChangefeedCursor>,
    principal_names: BTreeMap<String, String>,
    artifacts: BTreeMap<String, ArtifactOccurrenceRecord>,
    blob_lengths: BTreeMap<String, i64>,
    blob_artifacts: BTreeMap<String, String>,
    grants: BTreeMap<String, (String, ArtifactGrantSummary)>,
    links: BTreeMap<String, (String, ArtifactShareLinkSummary)>,
    agents: BTreeMap<String, AgentRecord>,
    wakes: BTreeMap<String, (String, bool)>,
    recordings: BTreeMap<String, RecordingRecord>,
    segments: BTreeMap<String, (String, i64)>,
}

impl ConsoleStreamState {
    async fn seed(
        state: &AdminState,
        tenant: &RecordId,
        cursor: ChangefeedCursor,
    ) -> anyhow::Result<Self> {
        let projection = load_projection(state, tenant).await?;
        let now = Utc::now();
        let principal_names = projection
            .principals
            .iter()
            .map(|principal| Ok((record_key(&principal.id)?, principal.display_name.clone())))
            .collect::<anyhow::Result<_>>()?;
        let mut artifacts = BTreeMap::new();
        let mut blob_artifacts = BTreeMap::new();
        for artifact in projection.artifacts {
            let id = record_key(&artifact.id)?;
            blob_artifacts.insert(record_key(&artifact.blob)?, id.clone());
            artifacts.insert(id, artifact);
        }
        let blob_lengths = projection
            .blobs
            .iter()
            .map(|blob| Ok((record_key(&blob.id)?, blob.byte_len)))
            .collect::<anyhow::Result<_>>()?;
        let mut grants = BTreeMap::new();
        for grant in &projection.grants {
            let artifact = record_key(&grant.r#in)?;
            if artifacts.contains_key(&artifact) {
                grants.insert(
                    record_key(&grant.id)?,
                    (artifact, artifact_grant_summary(grant)),
                );
            }
        }
        let mut links = BTreeMap::new();
        for link in &projection.share_links {
            let artifact = record_key(&link.artifact)?;
            if artifacts.contains_key(&artifact) {
                links.insert(
                    record_key(&link.id)?,
                    (artifact, share_link_summary(link, now)?),
                );
            }
        }
        let mut agents = BTreeMap::new();
        for agent in projection.agents {
            agents.insert(record_key(&agent.id)?, agent);
        }
        let mut wakes = BTreeMap::new();
        for wake in &projection.wakes {
            wakes.insert(
                record_key(&wake.id)?,
                (
                    record_key(&wake.agent)?,
                    matches!(wake.state, veoveo_platform_store::WakeState::Pending),
                ),
            );
        }
        let mut recordings = BTreeMap::new();
        for recording in projection.recordings {
            recordings.insert(record_key(&recording.id)?, recording);
        }
        let mut segments = BTreeMap::new();
        for segment in &projection.segments {
            segments.insert(
                record_key(&segment.id)?,
                (record_key(&segment.recording)?, segment.byte_len),
            );
        }
        Ok(Self {
            tenant: tenant.clone(),
            cursors: STREAM_TABLES
                .iter()
                .map(|table| (table_rank(*table), cursor))
                .collect(),
            principal_names,
            artifacts,
            blob_lengths,
            blob_artifacts,
            grants,
            links,
            agents,
            wakes,
            recordings,
            segments,
        })
    }

    async fn drain(&mut self, store: &PlatformStore) -> anyhow::Result<Vec<OutEvent>> {
        let mut events = Vec::new();
        for table in STREAM_TABLES {
            let rank = table_rank(table);
            let cursor = self.cursors[&rank];
            loop {
                let batches = store
                    .replay_changes(table, cursor, REPLAY_PAGE_LIMIT)
                    .await?;
                let Some(last) = batches.last().map(|batch| batch.versionstamp) else {
                    break;
                };
                let page_full = batches.len() == REPLAY_PAGE_LIMIT as usize;
                for batch in &batches {
                    for change in &batch.changes {
                        let entry = decode_changefeed_entry(change)?;
                        if let Some(event) = self.apply(table, rank, batch.versionstamp, entry)? {
                            events.push(event);
                        }
                    }
                }
                let advanced =
                    ChangefeedCursor::from_versionstamp(last.saturating_add(1)).unwrap_or(cursor);
                self.cursors.insert(rank, advanced);
                if !page_full {
                    break;
                }
            }
        }
        Ok(events)
    }

    fn apply(
        &mut self,
        table: PlatformTable,
        rank: usize,
        versionstamp: i64,
        entry: ChangefeedEntry,
    ) -> anyhow::Result<Option<OutEvent>> {
        let out = |name: &'static str, payload: serde_json::Value| {
            Some(OutEvent {
                versionstamp,
                rank,
                name,
                payload,
            })
        };
        match entry {
            ChangefeedEntry::Definition => Ok(None),
            ChangefeedEntry::Upsert(row) => {
                if !self.row_in_tenant(table, &row) {
                    return Ok(None);
                }
                match table {
                    PlatformTable::Principal => {
                        let principal: PrincipalRecord = row.into_t()?;
                        self.principal_names
                            .insert(record_key(&principal.id)?, principal.display_name);
                        Ok(None)
                    }
                    PlatformTable::Task => {
                        let task: TaskRecord = row.into_t()?;
                        let summary = task_summary(task, &self.principal_names)?;
                        Ok(out("task", upsert_payload(&summary)?))
                    }
                    PlatformTable::ArtifactBlob => {
                        let blob: ArtifactBlobRecord = row.into_t()?;
                        let key = record_key(&blob.id)?;
                        self.blob_lengths.insert(key.clone(), blob.byte_len);
                        match self.blob_artifacts.get(&key).cloned() {
                            Some(artifact) => self.emit_artifact(&artifact, versionstamp, rank),
                            None => Ok(None),
                        }
                    }
                    PlatformTable::ArtifactOccurrence => {
                        let artifact: ArtifactOccurrenceRecord = row.into_t()?;
                        let id = record_key(&artifact.id)?;
                        self.blob_artifacts
                            .insert(record_key(&artifact.blob)?, id.clone());
                        self.artifacts.insert(id.clone(), artifact);
                        self.emit_artifact(&id, versionstamp, rank)
                    }
                    PlatformTable::ArtifactGrant => {
                        let grant: ArtifactGrantEdge = row.into_t()?;
                        let artifact = record_key(&grant.r#in)?;
                        if !self.artifacts.contains_key(&artifact) {
                            return Ok(None);
                        }
                        self.grants.insert(
                            record_key(&grant.id)?,
                            (artifact.clone(), artifact_grant_summary(&grant)),
                        );
                        self.emit_artifact(&artifact, versionstamp, rank)
                    }
                    PlatformTable::ShareLink => {
                        let link: ShareLinkRecord = row.into_t()?;
                        let artifact = record_key(&link.artifact)?;
                        if !self.artifacts.contains_key(&artifact) {
                            return Ok(None);
                        }
                        self.links.insert(
                            record_key(&link.id)?,
                            (artifact.clone(), share_link_summary(&link, Utc::now())?),
                        );
                        self.emit_artifact(&artifact, versionstamp, rank)
                    }
                    PlatformTable::Agent => {
                        let agent: AgentRecord = row.into_t()?;
                        let id = record_key(&agent.id)?;
                        self.agents.insert(id.clone(), agent);
                        self.emit_agent(&id, versionstamp, rank)
                    }
                    PlatformTable::Wake => {
                        let wake: WakeRecord = row.into_t()?;
                        let agent = record_key(&wake.agent)?;
                        self.wakes.insert(
                            record_key(&wake.id)?,
                            (
                                agent.clone(),
                                matches!(wake.state, veoveo_platform_store::WakeState::Pending),
                            ),
                        );
                        self.emit_agent(&agent, versionstamp, rank)
                    }
                    PlatformTable::Recording => {
                        let recording: RecordingRecord = row.into_t()?;
                        let id = record_key(&recording.id)?;
                        self.recordings.insert(id.clone(), recording);
                        self.emit_recording(&id, versionstamp, rank)
                    }
                    PlatformTable::Segment => {
                        let segment: SegmentRecord = row.into_t()?;
                        let recording = record_key(&segment.recording)?;
                        self.segments.insert(
                            record_key(&segment.id)?,
                            (recording.clone(), segment.byte_len),
                        );
                        self.emit_recording(&recording, versionstamp, rank)
                    }
                    PlatformTable::AuditEvent => {
                        let event: AuditEventRecord = row.into_t()?;
                        let summary = audit_summary(event, &self.principal_names)?;
                        Ok(out("audit", upsert_payload(&summary)?))
                    }
                    _ => Ok(None),
                }
            }
            ChangefeedEntry::Delete { record, original } => {
                // INCLUDE ORIGINAL deletes carry the full prior row; a delete
                // whose original is missing or foreign-tenant is dropped.
                let Some(original) = original else {
                    return Ok(None);
                };
                if !self.row_in_tenant(table, &original) {
                    return Ok(None);
                }
                let key = record_key(&record)?;
                match table {
                    PlatformTable::Principal => {
                        self.principal_names.remove(&key);
                        Ok(None)
                    }
                    PlatformTable::Task => Ok(out("task", delete_payload(&key))),
                    PlatformTable::ArtifactBlob => {
                        self.blob_lengths.remove(&key);
                        Ok(None)
                    }
                    PlatformTable::ArtifactOccurrence => {
                        if let Some(artifact) = self.artifacts.remove(&key) {
                            if let Ok(blob) = record_key(&artifact.blob) {
                                self.blob_artifacts.remove(&blob);
                            }
                        }
                        self.grants.retain(|_, (artifact, _)| *artifact != key);
                        self.links.retain(|_, (artifact, _)| *artifact != key);
                        Ok(out("artifact", delete_payload(&key)))
                    }
                    PlatformTable::ArtifactGrant => match self.grants.remove(&key) {
                        Some((artifact, _)) => self.emit_artifact(&artifact, versionstamp, rank),
                        None => Ok(None),
                    },
                    PlatformTable::ShareLink => match self.links.remove(&key) {
                        Some((artifact, _)) => self.emit_artifact(&artifact, versionstamp, rank),
                        None => Ok(None),
                    },
                    PlatformTable::Agent => {
                        self.agents.remove(&key);
                        self.wakes.retain(|_, (agent, _)| *agent != key);
                        Ok(out("agent", delete_payload(&key)))
                    }
                    PlatformTable::Wake => match self.wakes.remove(&key) {
                        Some((agent, _)) => self.emit_agent(&agent, versionstamp, rank),
                        None => Ok(None),
                    },
                    PlatformTable::Recording => {
                        self.recordings.remove(&key);
                        self.segments.retain(|_, (recording, _)| *recording != key);
                        Ok(out("recording", delete_payload(&key)))
                    }
                    PlatformTable::Segment => match self.segments.remove(&key) {
                        Some((recording, _)) => self.emit_recording(&recording, versionstamp, rank),
                        None => Ok(None),
                    },
                    PlatformTable::AuditEvent => Ok(None),
                    _ => Ok(None),
                }
            }
        }
    }

    /// Grant edges carry no tenant field; membership is decided by whether
    /// the artifact they attach to is part of this tenant's state.
    fn row_in_tenant(&self, table: PlatformTable, row: &DbValue) -> bool {
        if matches!(table, PlatformTable::ArtifactGrant) {
            return true;
        }
        matches!(row.get("tenant"), DbValue::RecordId(record) if *record == self.tenant)
    }

    fn emit_artifact(
        &self,
        id: &str,
        versionstamp: i64,
        rank: usize,
    ) -> anyhow::Result<Option<OutEvent>> {
        let Some(artifact) = self.artifacts.get(id) else {
            return Ok(None);
        };
        let byte_length = record_key(&artifact.blob)
            .ok()
            .and_then(|blob| self.blob_lengths.get(&blob).copied())
            .unwrap_or(0);
        let mut grants: Vec<_> = self
            .grants
            .values()
            .filter(|(artifact, _)| artifact == id)
            .map(|(_, grant)| grant.clone())
            .collect();
        grants.sort_by_key(|grant| std::cmp::Reverse(grant.created_at));
        let mut links: Vec<_> = self
            .links
            .values()
            .filter(|(artifact, _)| artifact == id)
            .map(|(_, link)| link.clone())
            .collect();
        links.sort_by_key(|link| std::cmp::Reverse(link.created_at));
        let summary = artifact_summary(
            artifact.clone(),
            byte_length,
            grants,
            links,
            &self.principal_names,
        )?;
        Ok(Some(OutEvent {
            versionstamp,
            rank,
            name: "artifact",
            payload: upsert_payload(&summary)?,
        }))
    }

    fn emit_agent(
        &self,
        id: &str,
        versionstamp: i64,
        rank: usize,
    ) -> anyhow::Result<Option<OutEvent>> {
        let Some(agent) = self.agents.get(id) else {
            return Ok(None);
        };
        let pending = self
            .wakes
            .values()
            .filter(|(agent, pending)| agent == id && *pending)
            .count();
        let summary = agent_summary(agent.clone(), pending)?;
        Ok(Some(OutEvent {
            versionstamp,
            rank,
            name: "agent",
            payload: upsert_payload(&summary)?,
        }))
    }

    fn emit_recording(
        &self,
        id: &str,
        versionstamp: i64,
        rank: usize,
    ) -> anyhow::Result<Option<OutEvent>> {
        let Some(recording) = self.recordings.get(id) else {
            return Ok(None);
        };
        let (count, bytes) = self
            .segments
            .values()
            .filter(|(recording, _)| recording == id)
            .fold((0usize, 0i64), |(count, total), (_, bytes)| {
                (count + 1, total + bytes)
            });
        let summary = recording_summary(recording.clone(), count, bytes)?;
        Ok(Some(OutEvent {
            versionstamp,
            rank,
            name: "recording",
            payload: upsert_payload(&summary)?,
        }))
    }
}

fn upsert_payload<T: serde::Serialize>(row: &T) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::json!({ "op": "upsert", "row": serde_json::to_value(row)? }))
}

fn delete_payload(id: &str) -> serde_json::Value {
    serde_json::json!({ "op": "delete", "id": id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_tables_have_unique_ranks_in_dependency_order() {
        for (index, table) in STREAM_TABLES.iter().enumerate() {
            assert_eq!(table_rank(*table), index);
        }
        assert!(table_rank(PlatformTable::Principal) < table_rank(PlatformTable::Task));
        assert!(
            table_rank(PlatformTable::ArtifactBlob) < table_rank(PlatformTable::ArtifactOccurrence)
        );
        assert!(
            table_rank(PlatformTable::ArtifactOccurrence)
                < table_rank(PlatformTable::ArtifactGrant)
        );
        assert!(table_rank(PlatformTable::Agent) < table_rank(PlatformTable::Wake));
        assert!(table_rank(PlatformTable::Recording) < table_rank(PlatformTable::Segment));
    }

    #[test]
    fn group_boundaries_attach_ids_to_the_last_event_of_each_versionstamp() {
        let events = vec![
            OutEvent {
                versionstamp: 100,
                rank: 1,
                name: "task",
                payload: serde_json::json!({}),
            },
            OutEvent {
                versionstamp: 100,
                rank: 3,
                name: "artifact",
                payload: serde_json::json!({}),
            },
            OutEvent {
                versionstamp: 200,
                rank: 1,
                name: "task",
                payload: serde_json::json!({}),
            },
        ];
        let rendered = group_events(events);
        assert_eq!(rendered.len(), 3);
        // Event doesn't expose its fields; assert through serialization.
        let frames: Vec<String> = rendered
            .into_iter()
            .map(|event| format!("{event:?}"))
            .collect();
        assert!(
            !frames[0].contains("\"100\""),
            "first event of group has no id: {}",
            frames[0]
        );
        assert!(
            frames[1].contains("100"),
            "group tail carries the id: {}",
            frames[1]
        );
        assert!(
            frames[2].contains("200"),
            "single-event group carries the id: {}",
            frames[2]
        );
    }

    #[test]
    fn delete_payloads_carry_only_the_record_key() {
        let payload = delete_payload("0197f78e");
        assert_eq!(payload["op"], "delete");
        assert_eq!(payload["id"], "0197f78e");
        assert!(payload.get("row").is_none());
    }
}
