use std::{
    collections::BTreeMap,
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};
use veoveo_mcp_contract::audit::AdministrativeOperation;

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
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tokio_util::sync::CancellationToken;
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{
    AgentRecord, ArtifactAccessRequestRecord, ArtifactBlobRecord, ArtifactGrantEdge,
    ArtifactOccurrenceRecord, ArtifactUploadRecord, ArtifactUploadState, ChangefeedConsumerId,
    ChangefeedCursor, ChangefeedDelivery, ChangefeedEntry, ObservationTable, PlatformStore,
    PrincipalRecord, RecordId, RecordingLayerRecord, RecordingLayerState, RecordingRecord,
    ShareLinkRecord, TaskRecord, Value as DbValue, WakeRecord, decode_changefeed_entry,
    deterministic_tenant_id,
};

use super::projection::{
    ArtifactAccessContext, ArtifactGrantSummary, ArtifactShareLinkSummary, agent_public_key,
    agent_summary, artifact_grant_summary, artifact_summary, load_projection, principal_summary,
    record_key, recording_summary, server_summary, share_link_summary, task_summary,
};
use crate::{
    admin::admin_profile_id,
    audit::authorize_admin_request,
    runtime::{AdminState, current_catalog},
};

const MAX_CONCURRENT_STREAMS: usize = 16;
const MAX_STREAMS_PER_PRINCIPAL: usize = 3;
const MAX_STREAM_LIFETIME: Duration = Duration::from_secs(15 * 60);
const WAKE_DEBOUNCE: Duration = Duration::from_millis(200);
const REPLAY_PAGE_LIMIT: u32 = 1_000;
const RETRY_HINT_MS: u32 = 3_000;
/// Cursors older than this force a full resync instead of replaying a huge
/// backlog; a browser away that long should refetch the snapshot anyway.
const REPLAY_HORIZON: chrono::TimeDelta = chrono::TimeDelta::hours(24);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConsoleTable {
    Principal,
    Task,
    ArtifactBlob,
    ArtifactOccurrence,
    ArtifactUpload,
    ArtifactGrant,
    ArtifactAccessRequest,
    ShareLink,
    Agent,
    Wake,
    Recording,
    RecordingLayer,
}
impl ConsoleTable {
    fn observation(self) -> ObservationTable {
        use veoveo_agent_runtime::AgentObservationTable as Agents;
        use veoveo_platform_store::PlatformTable as Kernel;
        use veoveo_recording_mcp::schema::RecordingObservationTable as Recordings;
        match self {
            Self::Principal => Kernel::Principal.into(),
            Self::Task => Kernel::Task.into(),
            Self::ArtifactBlob => Kernel::ArtifactBlob.into(),
            Self::ArtifactOccurrence => Kernel::ArtifactOccurrence.into(),
            Self::ArtifactUpload => Kernel::ArtifactUpload.into(),
            Self::ArtifactGrant => Kernel::ArtifactGrant.into(),
            Self::ArtifactAccessRequest => Kernel::ArtifactAccessRequest.into(),
            Self::ShareLink => Kernel::ShareLink.into(),
            Self::Agent => Agents::Agent.into(),
            Self::Wake => Agents::Wake.into(),
            Self::Recording => Recordings::Recording.into(),
            Self::RecordingLayer => Recordings::Layer.into(),
        }
    }
}

/// Tenant tables the console stream follows, in dependency order: within one
/// versionstamp group parents apply before the children that re-emit them.
const STREAM_TABLES: [ConsoleTable; 12] = [
    ConsoleTable::Principal,
    ConsoleTable::Task,
    ConsoleTable::ArtifactBlob,
    ConsoleTable::ArtifactOccurrence,
    ConsoleTable::ArtifactUpload,
    ConsoleTable::ArtifactGrant,
    ConsoleTable::ArtifactAccessRequest,
    ConsoleTable::ShareLink,
    ConsoleTable::Agent,
    ConsoleTable::Wake,
    ConsoleTable::Recording,
    ConsoleTable::RecordingLayer,
];

const fn table_rank(table: ConsoleTable) -> usize {
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

pub(super) struct StreamSlot {
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
    pub(super) fn acquire(&self, principal: &str) -> Option<StreamSlot> {
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
    tokio::spawn(async move {
        use futures::StreamExt;
        let replica = std::env::var("HOSTNAME").unwrap_or_else(|_| "local".into());
        let consumer = ChangefeedConsumerId::new(format!("gateway-console/{replica}"))
            .expect("gateway replica must be a valid consumer identity");
        let cursor = store
            .changefeed_checkpoint(&consumer)
            .await
            .unwrap_or_default();
        let mut source = store.observe_changes(
            STREAM_TABLES
                .into_iter()
                .map(ConsoleTable::observation)
                .collect(),
            cursor,
        );
        loop {
            let delivery = tokio::select! {
                () = cancellation.cancelled() => return,
                delivery = source.next() => delivery,
            };
            match delivery {
                Some(Ok(delivery)) => {
                    let changed = match &delivery {
                        ChangefeedDelivery::Reconcile { .. } => true,
                        ChangefeedDelivery::Changes { entries, .. } => !entries.is_empty(),
                    };
                    if changed {
                        wake_tx.send_modify(|epoch| *epoch = epoch.wrapping_add(1));
                    }
                    if let Err(error) = store.checkpoint_changes(&consumer, delivery.cursor()).await
                    {
                        tracing::warn!(%error, "console feed checkpoint unavailable");
                    }
                }
                Some(Err(error)) => {
                    tracing::warn!(%error, "console native feed reconnecting");
                    wake_tx.send_modify(|epoch| *epoch = epoch.wrapping_add(1));
                }
                None => return,
            }
        }
    });
    ConsoleStreamRuntime {
        wake: wake_rx,
        limits: Arc::new(StreamLimits {
            global: Arc::new(Semaphore::new(MAX_CONCURRENT_STREAMS)),
            per_principal: Mutex::new(BTreeMap::new()),
        }),
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
        AdministrativeOperation::ConsoleStream,
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
        Err(response) => return *response,
    };

    let deadline = stream_deadline(subject.access_token.expires_at);
    let artifact_access = match ArtifactAccessContext::from_subject(&subject, &tenant_key) {
        Ok(access) => access,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let stream = console_event_stream(
        state,
        store,
        tenant,
        cursor,
        slot,
        deadline,
        artifact_access,
    );
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
) -> Result<ChangefeedCursor, Box<Response>> {
    match requested {
        Some(raw) => {
            let cursor = raw
                .parse::<i64>()
                .ok()
                .and_then(ChangefeedCursor::from_versionstamp);
            let Some(cursor) = cursor else {
                return Err(StatusCode::BAD_REQUEST.into_response().into());
            };
            // A cursor beyond the replay horizon forces a resync: the reset
            // event tells the client to refetch the snapshot rather than
            // replay an unbounded backlog.
            let implied_ms = cursor.versionstamp() >> 16;
            let horizon = Utc::now() - REPLAY_HORIZON;
            if cursor.versionstamp() > 0 && implied_ms < horizon.timestamp_millis() {
                return Err(reset_response("cursor-out-of-range").into());
            }
            Ok(cursor)
        }
        None => store.changefeed_cursor_now().await.map_err(|error| {
            tracing::error!("console stream cursor anchor failed: {error}");
            Box::new(StatusCode::INTERNAL_SERVER_ERROR.into_response())
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
    artifact_access: ArtifactAccessContext,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        // Owned by the generator so the limit slot lives exactly as long as
        // the response stream.
        let _slot = slot;
        let mut wake = state.console_stream.wake.clone();
        let mut health_epoch = state.server_health.epoch.clone();
        wake.mark_changed();

        yield Ok(Event::default().retry(Duration::from_millis(RETRY_HINT_MS as u64)));

        let mut projection_state = match ConsoleStreamState::seed(
            &state,
            &tenant,
            cursor,
            artifact_access,
        ).await {
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
    cursor: ChangefeedCursor,
    principal_names: BTreeMap<String, String>,
    artifacts: BTreeMap<String, ArtifactOccurrenceRecord>,
    blob_lengths: BTreeMap<String, i64>,
    blob_artifacts: BTreeMap<String, String>,
    grants: BTreeMap<String, (String, ArtifactGrantSummary)>,
    links: BTreeMap<String, (String, ArtifactShareLinkSummary)>,
    agents: BTreeMap<String, AgentRecord>,
    wakes: BTreeMap<String, (String, bool)>,
    recordings: BTreeMap<String, RecordingRecord>,
    layers: BTreeMap<String, (String, i64, RecordingLayerState)>,
    artifact_access: ArtifactAccessContext,
}

#[derive(Serialize)]
struct UploadChanged {
    op: &'static str,
    upload_id: veoveo_mcp_contract::ArtifactUploadId,
    state: ArtifactUploadState,
}

impl ConsoleStreamState {
    async fn seed(
        state: &AdminState,
        tenant: &RecordId,
        cursor: ChangefeedCursor,
        artifact_access: ArtifactAccessContext,
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
        let mut layers = BTreeMap::new();
        for layer in &projection.layers {
            layers.insert(
                record_key(&layer.id)?,
                (record_key(&layer.recording)?, layer.byte_len, layer.state),
            );
        }
        Ok(Self {
            tenant: tenant.clone(),
            cursor,
            principal_names,
            artifacts,
            blob_lengths,
            blob_artifacts,
            grants,
            links,
            agents,
            wakes,
            recordings,
            layers,
            artifact_access,
        })
    }

    async fn drain(&mut self, store: &PlatformStore) -> anyhow::Result<Vec<OutEvent>> {
        let mut events = Vec::new();
        // Bound each round so sustained writes still yield feedback to clients.
        for _ in 0..4 {
            let batches = store.replay_changes(self.cursor, REPLAY_PAGE_LIMIT).await?;
            let Some(last) = batches.last().map(|batch| batch.versionstamp) else {
                break;
            };
            for batch in batches {
                let mut entries = Vec::new();
                for change in &batch.changes {
                    let entry = decode_changefeed_entry(change)?;
                    let Some(table) = STREAM_TABLES
                        .iter()
                        .copied()
                        .find(|table| entry.table() == Some(table.observation().as_str()))
                    else {
                        continue;
                    };
                    entries.push((table, entry));
                }
                entries.sort_by_key(|(table, _)| table_rank(*table));
                for (table, entry) in entries {
                    if table == ConsoleTable::ArtifactOccurrence {
                        self.resolve_referenced_blob(store, &entry).await?;
                    }
                    if let Some(event) =
                        self.apply(table, table_rank(table), batch.versionstamp, entry)?
                    {
                        events.push(event);
                    }
                }
            }
            self.cursor =
                ChangefeedCursor::from_versionstamp(last.saturating_add(1)).unwrap_or(self.cursor);
        }
        Ok(events)
    }

    /// A new occurrence can reuse a blob older than the initial Console window.
    async fn resolve_referenced_blob(
        &mut self,
        store: &PlatformStore,
        entry: &ChangefeedEntry,
    ) -> anyhow::Result<()> {
        if let ChangefeedEntry::Upsert(row) = entry {
            if !self.row_in_tenant(ConsoleTable::ArtifactOccurrence, row) {
                return Ok(());
            }
            let artifact: ArtifactOccurrenceRecord = row.clone().into_t()?;
            let key = record_key(&artifact.blob)?;
            if !self.blob_lengths.contains_key(&key) {
                let blob: Option<ArtifactBlobRecord> = store.client().select(artifact.blob).await?;
                if let Some(blob) = blob
                    && blob.tenant == self.tenant
                {
                    self.blob_lengths.insert(key, blob.byte_len);
                }
            }
        }
        Ok(())
    }

    fn apply(
        &mut self,
        table: ConsoleTable,
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
                    ConsoleTable::Principal => {
                        let principal: PrincipalRecord = row.into_t()?;
                        let summary = principal_summary(&principal);
                        self.principal_names
                            .insert(record_key(&principal.id)?, principal.display_name);
                        Ok(out("principal", upsert_payload(&summary)?))
                    }
                    ConsoleTable::Task => {
                        let task: TaskRecord = row.into_t()?;
                        let summary = task_summary(task, &self.principal_names)?;
                        Ok(out("task", upsert_payload(&summary)?))
                    }
                    ConsoleTable::ArtifactBlob => {
                        let blob: ArtifactBlobRecord = row.into_t()?;
                        let key = record_key(&blob.id)?;
                        self.blob_lengths.insert(key.clone(), blob.byte_len);
                        match self.blob_artifacts.get(&key).cloned() {
                            Some(artifact) => self.emit_artifact(&artifact, versionstamp, rank),
                            None => Ok(None),
                        }
                    }
                    ConsoleTable::ArtifactOccurrence => {
                        let artifact: ArtifactOccurrenceRecord = row.into_t()?;
                        let id = record_key(&artifact.id)?;
                        self.blob_artifacts
                            .insert(record_key(&artifact.blob)?, id.clone());
                        self.artifacts.insert(id.clone(), artifact);
                        self.emit_artifact(&id, versionstamp, rank)
                    }
                    ConsoleTable::ArtifactUpload => {
                        let upload: ArtifactUploadRecord = row.into_t()?;
                        if upload.state == ArtifactUploadState::Open
                            || !self.artifact_access.matches_upload_scope(
                                &upload.tenant_key,
                                &upload.actor_key,
                                &upload.authority.context_key,
                            )
                        {
                            return Ok(None);
                        }
                        // Contentless, scoped notifications trigger a currently authorized status
                        // read. File descriptors, storage handles, and authority never enter SSE.
                        let event = UploadChanged {
                            op: "changed",
                            upload_id: veoveo_mcp_contract::ArtifactUploadId::parse(record_key(
                                &upload.id,
                            )?)?,
                            state: upload.state,
                        };
                        Ok(out("artifact_upload", serde_json::to_value(event)?))
                    }
                    ConsoleTable::ArtifactGrant => {
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
                    ConsoleTable::ArtifactAccessRequest => {
                        let request: ArtifactAccessRequestRecord = row.into_t()?;
                        Ok(out(
                            "access_request",
                            serde_json::json!({
                                "op": "changed",
                                "id": record_key(&request.id)?,
                            }),
                        ))
                    }
                    ConsoleTable::ShareLink => {
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
                    ConsoleTable::Agent => {
                        let agent: AgentRecord = row.into_t()?;
                        let id = record_key(&agent.id)?;
                        self.agents.insert(id.clone(), agent);
                        self.emit_agent(&id, versionstamp, rank)
                    }
                    ConsoleTable::Wake => {
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
                    ConsoleTable::Recording => {
                        let recording: RecordingRecord = row.into_t()?;
                        let id = record_key(&recording.id)?;
                        self.recordings.insert(id.clone(), recording);
                        self.emit_recording(&id, versionstamp, rank)
                    }
                    ConsoleTable::RecordingLayer => {
                        let layer: RecordingLayerRecord = row.into_t()?;
                        let recording = record_key(&layer.recording)?;
                        self.layers.insert(
                            record_key(&layer.id)?,
                            (recording.clone(), layer.byte_len, layer.state),
                        );
                        self.emit_recording(&recording, versionstamp, rank)
                    }
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
                    ConsoleTable::Principal => {
                        let principal: PrincipalRecord = original.into_t()?;
                        self.principal_names.remove(&key);
                        Ok(out(
                            "principal",
                            delete_payload(&principal_summary(&principal).id),
                        ))
                    }
                    ConsoleTable::Task => Ok(out("task", delete_payload(&key))),
                    ConsoleTable::ArtifactBlob => {
                        self.blob_lengths.remove(&key);
                        Ok(None)
                    }
                    ConsoleTable::ArtifactOccurrence => {
                        if let Some(artifact) = self.artifacts.remove(&key)
                            && let Ok(blob) = record_key(&artifact.blob)
                        {
                            self.blob_artifacts.remove(&blob);
                        }
                        self.grants.retain(|_, (artifact, _)| *artifact != key);
                        self.links.retain(|_, (artifact, _)| *artifact != key);
                        Ok(out("artifact", delete_payload(&key)))
                    }
                    ConsoleTable::ArtifactGrant => match self.grants.remove(&key) {
                        Some((artifact, _)) => self.emit_artifact(&artifact, versionstamp, rank),
                        None => Ok(None),
                    },
                    ConsoleTable::ArtifactAccessRequest => Ok(out(
                        "access_request",
                        serde_json::json!({ "op": "changed", "id": key }),
                    )),
                    ConsoleTable::ShareLink => match self.links.remove(&key) {
                        Some((artifact, _)) => self.emit_artifact(&artifact, versionstamp, rank),
                        None => Ok(None),
                    },
                    ConsoleTable::Agent => {
                        let agent: AgentRecord = original.into_t()?;
                        self.agents.remove(&key);
                        self.wakes.retain(|_, (agent, _)| *agent != key);
                        Ok(out("agent", delete_payload(agent_public_key(&agent))))
                    }
                    ConsoleTable::Wake => match self.wakes.remove(&key) {
                        Some((agent, _)) => self.emit_agent(&agent, versionstamp, rank),
                        None => Ok(None),
                    },
                    ConsoleTable::Recording => {
                        self.recordings.remove(&key);
                        self.layers.retain(|_, (recording, _, _)| *recording != key);
                        Ok(out("recording", delete_payload(&key)))
                    }
                    ConsoleTable::RecordingLayer => match self.layers.remove(&key) {
                        Some((recording, _, _)) => {
                            self.emit_recording(&recording, versionstamp, rank)
                        }
                        None => Ok(None),
                    },
                    _ => Ok(None),
                }
            }
        }
    }

    /// Grant edges carry no tenant field; membership is decided by whether
    /// the artifact they attach to is part of this tenant's state.
    fn row_in_tenant(&self, table: ConsoleTable, row: &DbValue) -> bool {
        if matches!(table, ConsoleTable::ArtifactGrant) {
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
            .and_then(|blob| self.blob_lengths.get(&blob).copied());
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
            &self.artifact_access,
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
        let (layer_count, committed_layer_count, committed_byte_length) = self
            .layers
            .values()
            .filter(|(recording, _, _)| recording == id)
            .fold(
                (0usize, 0usize, 0i64),
                |(total, playable, bytes), (_, byte_len, state)| {
                    if *state == RecordingLayerState::Committed {
                        (total + 1, playable + 1, bytes + byte_len)
                    } else {
                        (total + 1, playable, bytes)
                    }
                },
            );
        let summary = recording_summary(
            recording.clone(),
            layer_count,
            committed_layer_count,
            committed_byte_length,
        )?;
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
        assert!(table_rank(ConsoleTable::Principal) < table_rank(ConsoleTable::Task));
        assert!(
            table_rank(ConsoleTable::ArtifactBlob) < table_rank(ConsoleTable::ArtifactOccurrence)
        );
        assert!(
            table_rank(ConsoleTable::ArtifactOccurrence) < table_rank(ConsoleTable::ArtifactGrant)
        );
        assert!(table_rank(ConsoleTable::Agent) < table_rank(ConsoleTable::Wake));
        assert!(table_rank(ConsoleTable::Recording) < table_rank(ConsoleTable::RecordingLayer));
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
