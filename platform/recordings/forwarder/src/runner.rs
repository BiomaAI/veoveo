use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use re_byte_size::SizeBytes as _;
use re_grpc_server::{MemoryLimit, PlaybackBehavior, ServerOptions, shutdown};
use re_log_channel::DataSourceMessage;
use re_log_types::{LogMsg, StoreId, StoreKind};
use reqwest::header::{HOST, HeaderMap, HeaderValue};
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use veoveo_recording_protocol::v1::{
    IngestErrorCode, OpenRecordingStreamRequest, RecordingIngestQuota, RecordingStreamFinishMode,
};

use crate::{
    batch::{BatchBoundary, RecordingAccumulator},
    blueprint::{BlueprintAccumulator, associated_recording},
    client::{IngestRequestError, RecordingIngestClient},
    config::ForwarderConfig,
    oauth::{OAuthTokenProvider, OAuthTokenProviderConfig},
    queue::{DurableQueue, QueueDiagnostics, QueueFull},
};

const MAXIMUM_INCOMPLETE_BLUEPRINT_STORES: usize = 32;

#[derive(Clone, Copy)]
struct RerunIngestLimits {
    batch_bytes: u64,
    blueprint_bytes: u64,
    blueprint_messages: u64,
    batch_messages: usize,
    maximum_batch_source_span: Duration,
}

#[derive(Debug, Default)]
struct QueueEvents {
    work_available: Notify,
    capacity_available: Notify,
    receiver_buffered_messages: AtomicUsize,
    volatile_batches: AtomicUsize,
    volatile_blueprints: AtomicUsize,
}

pub async fn run(config: ForwarderConfig) -> Result<()> {
    config.validate()?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
    let mut headers = HeaderMap::new();
    headers.insert(
        HOST,
        HeaderValue::from_str(&canonical_authority(&config.gateway_url)?)?,
    );
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .https_only(config.gateway_transport_url().scheme() == "https")
        .connect_timeout(config.request_timeout())
        .timeout(config.request_timeout())
        .build()?;
    let private_key_pem_file = config.private_key_pem_file.clone();
    let client_id = config.client_id.clone();
    let key_id = config.key_id.clone();
    let algorithm = config.signing_algorithm;
    let protected_resource = config.protected_resource.clone();
    let client = RecordingIngestClient::discover(
        http.clone(),
        &config.gateway_url,
        config.gateway_transport_url(),
        &config.protected_resource,
        move |token_endpoint, token_transport_endpoint| {
            OAuthTokenProvider::new(OAuthTokenProviderConfig {
                http,
                token_endpoint,
                token_transport_endpoint,
                protected_resource,
                client_id,
                scope: veoveo_recording_contract::RecordingProducerScope::Ingest,
                key_id,
                algorithm,
                private_key_pem_file,
            })
        },
    )
    .await?;
    ensure!(
        config.maximum_queue_bytes >= client.maximum_batch_bytes(),
        "durable queue must hold at least one maximum-size gateway batch"
    );
    let limits = RerunIngestLimits {
        batch_bytes: client.maximum_batch_bytes(),
        blueprint_bytes: client.maximum_blueprint_bytes(),
        blueprint_messages: client.maximum_blueprint_messages(),
        batch_messages: config.batch_message_limit,
        maximum_batch_source_span: config.maximum_batch_source_span(),
    };
    let queue = Arc::new(Mutex::new(DurableQueue::open(
        config.queue_dir.clone(),
        config.maximum_queue_bytes,
    )?));
    let queue_events = Arc::new(QueueEvents::default());
    let uploader_stop = CancellationToken::new();
    let mut uploader = tokio::spawn(upload_loop(
        queue.clone(),
        queue_events.clone(),
        client.clone(),
        uploader_stop.child_token(),
    ));

    let (grpc_stop_signal, grpc_shutdown) = shutdown::shutdown();
    let (receiver, grpc_handle) = re_grpc_server::spawn_with_recv(
        config.bind,
        ServerOptions {
            playback_behavior: PlaybackBehavior::NewestFirst,
            memory_limit: MemoryLimit::from_bytes(config.grpc_memory_limit_bytes),
            ..Default::default()
        },
        grpc_shutdown,
    );
    info!(bind = %config.bind, "recording forwarder loopback Rerun receiver up");
    // Collect contiguous Rerun delivery in the native bridge. The async consumer
    // yields between messages so shutdown stays serviceable. Batch boundaries use video
    // access units, monotonic source-generation span, message count, and bytes;
    // Rerun gRPC does not expose the SDK batcher's flush marker.
    let (message_tx, mut message_rx) = mpsc::channel::<Vec<LogMsg>>(64);
    let receiver_stop = CancellationToken::new();
    let mut receiver_task = spawn_receiver(
        receiver,
        message_tx,
        receiver_stop.clone(),
        queue_events.clone(),
    );

    let mut accumulators = HashMap::<StoreId, RecordingAccumulator>::new();
    let mut blueprint_accumulators = HashMap::<StoreId, BlueprintAccumulator>::new();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    let progress = Arc::new(Mutex::new(ShutdownProgress::default()));
    let work_progress = progress.clone();
    let mut receiver_joined = false;
    let mut uploader_joined = false;
    let (outcome, end) = {
        // Keep this future (including any encoded batch awaiting queue space)
        // owned while the signal is observed. No intake future is recreated.
        let work = async {
            while let Some(burst) = message_rx.recv().await {
                queue_events
                    .receiver_buffered_messages
                    .fetch_sub(burst.len(), Ordering::Relaxed);
                work_progress.lock().unwrap().burst_messages = burst.len();
                for message in burst {
                    tokio::task::yield_now().await;
                    handle_rerun_message(
                        message,
                        &mut accumulators,
                        &mut blueprint_accumulators,
                        &queue,
                        &queue_events,
                        limits,
                        config.finish_superseded_recordings,
                    )
                    .await?;
                    let mut state = work_progress.lock().unwrap();
                    state.burst_messages -= 1;
                    state.accumulator_messages = accumulators
                        .values()
                        .map(RecordingAccumulator::pending_len)
                        .sum();
                    state.incomplete_blueprint_stores = blueprint_accumulators.len();
                }
            }
            work_progress.lock().unwrap().stage = ShutdownStage::ReceiverJoin;
            let received = (&mut receiver_task).await;
            receiver_joined = true;
            received.context("Rerun receiver task panicked")??;
            work_progress.lock().unwrap().stage = ShutdownStage::Flush;
            flush_accumulators(
                &mut accumulators,
                &queue,
                &queue_events,
                client.maximum_batch_bytes(),
            )
            .await?;
            work_progress.lock().unwrap().accumulator_messages = 0;
            ensure!(
                blueprint_accumulators.is_empty(),
                "shutdown retained {} incomplete volatile Blueprint stores",
                blueprint_accumulators.len()
            );
            queue
                .lock()
                .expect("durable queue mutex poisoned")
                .request_finish_all()?;
            queue_events.work_available.notify_one();
            uploader_stop.cancel();
            work_progress.lock().unwrap().stage = ShutdownStage::UploaderJoin;
            let uploaded = (&mut uploader).await;
            uploader_joined = true;
            uploaded.context("recording uploader task panicked")??;
            work_progress.lock().unwrap().stage = ShutdownStage::UploadDrain;
            drain_and_finish(queue.clone(), &client).await
        };
        shutdown_work(
            work,
            &mut shutdown,
            config.shutdown_drain_window(),
            || {
                grpc_stop_signal.stop();
                drop(grpc_handle);
            },
            &progress,
            &queue_events,
        )
        .await
    };
    retire_jobs(
        outcome,
        &mut message_rx,
        RetiringJob {
            stop: receiver_stop,
            handle: &mut receiver_task,
            joined: receiver_joined,
        },
        RetiringJob {
            stop: uploader_stop,
            handle: &mut uploader,
            joined: uploader_joined,
        },
        end,
    )
    .await
}

fn spawn_receiver(
    receiver: re_log_channel::LogReceiver,
    message_tx: mpsc::Sender<Vec<LogMsg>>,
    worker_stop: CancellationToken,
    events: Arc<QueueEvents>,
) -> tokio::task::JoinHandle<Result<()>> {
    tokio::task::spawn_blocking(move || {
        while !worker_stop.is_cancelled() {
            let received = match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(received) => received,
                Err(re_log_channel::RecvTimeoutError::Timeout) => continue,
                Err(re_log_channel::RecvTimeoutError::Disconnected) => break,
            };
            let mut burst = Vec::with_capacity(256);
            if let Some(DataSourceMessage::LogMsg(message)) = received.into_data() {
                burst.push(message);
            }
            while burst.len() < 4_096 && !worker_stop.is_cancelled() {
                let Ok(received) = receiver.try_recv() else {
                    break;
                };
                if let Some(DataSourceMessage::LogMsg(message)) = received.into_data() {
                    burst.push(message);
                }
            }
            if !burst.is_empty() {
                events
                    .receiver_buffered_messages
                    .fetch_add(burst.len(), Ordering::Relaxed);
                if message_tx.blocking_send(burst).is_err() {
                    break;
                }
            }
        }
        Ok(())
    })
}

#[derive(Debug, Default)]
enum ShutdownStage {
    #[default]
    IntakeDrain,
    ReceiverJoin,
    Flush,
    UploaderJoin,
    UploadDrain,
}
#[derive(Default)]
struct ShutdownProgress {
    stage: ShutdownStage,
    burst_messages: usize,
    accumulator_messages: usize,
    incomplete_blueprint_stores: usize,
}

async fn shutdown_work(
    work: impl std::future::Future<Output = Result<()>>,
    signal: impl std::future::Future<Output = Result<()>>,
    window: Duration,
    stop_listener: impl FnOnce(),
    progress: &Mutex<ShutdownProgress>,
    events: &QueueEvents,
) -> (Result<()>, tokio::time::Instant) {
    tokio::pin!(work, signal);
    let trigger = tokio::select! {
        biased;
        result = &mut signal => Err(result),
        completed = &mut work => Ok(completed),
    };
    let end = tokio::time::Instant::now() + window;
    stop_listener();
    // The receiver's bounded read and closed send channel use this retirement
    // reserve inside the original shutdown interval, never a second deadline.
    let reserve = Duration::from_millis(250).min(window / 2);
    let work_end = end - reserve;
    let outcome = match trigger {
        Ok(completed) => completed,
        Err(Err(error)) => Err(error),
        Err(Ok(())) => match tokio::time::timeout_at(work_end, &mut work).await {
            Ok(result) => result,
            Err(_) => {
                let state = progress.lock().unwrap();
                Err(anyhow::anyhow!(
                    "forwarder shutdown deadline: stage={:?} volatile_burst_messages={} \
                     volatile_encoded_batches={} volatile_blueprints={} receiver_buffered_messages={} \
                     accumulator_messages={} incomplete_blueprint_stores={}; durable queue retained",
                    state.stage,
                    state.burst_messages,
                    events.volatile_batches.load(Ordering::Relaxed),
                    events.volatile_blueprints.load(Ordering::Relaxed),
                    events.receiver_buffered_messages.load(Ordering::Relaxed),
                    state.accumulator_messages,
                    state.incomplete_blueprint_stores,
                ))
            }
        },
    };
    // timeout_at polls the inner future first. Encoding or fsync can complete
    // in one poll after the original shutdown cap without yielding to its timer.
    let outcome = if tokio::time::Instant::now() > end {
        match outcome {
            Ok(()) => Err(anyhow::anyhow!(
                "forwarder shutdown work completed after original deadline"
            )),
            Err(error) => {
                Err(error.context("forwarder shutdown work completed after original deadline"))
            }
        }
    } else {
        outcome
    };
    (outcome, end)
}

struct RetiringJob<'a> {
    stop: CancellationToken,
    handle: &'a mut tokio::task::JoinHandle<Result<()>>,
    joined: bool,
}
impl RetiringJob<'_> {
    async fn join(self, end: tokio::time::Instant) -> Result<()> {
        ensure!(
            tokio::time::Instant::now() < end,
            "forwarder shutdown job retirement deadline expired"
        );
        if self.joined {
            return Ok(());
        }
        let result = tokio::time::timeout_at(end, self.handle)
            .await
            .context("forwarder shutdown job retirement deadline")?
            .context("forwarder shutdown job panicked")?;
        ensure!(
            tokio::time::Instant::now() <= end,
            "forwarder shutdown job retired after deadline"
        );
        result
    }
}
async fn retire_jobs(
    outcome: Result<()>,
    message_rx: &mut mpsc::Receiver<Vec<LogMsg>>,
    receiver: RetiringJob<'_>,
    uploader: RetiringJob<'_>,
    end: tokio::time::Instant,
) -> Result<()> {
    receiver.stop.cancel();
    message_rx.close(); // Releases blocking_send even when capacity is exhausted.
    uploader.stop.cancel();
    // Always attempt both original joins, including after work or join failure.
    let (receiver_result, uploader_result) = tokio::join!(receiver.join(end), uploader.join(end));
    let errors = [outcome.err(), receiver_result.err(), uploader_result.err()]
        .into_iter()
        .flatten()
        .map(|e| format!("{e:#}"))
        .collect::<Vec<_>>();
    ensure!(
        errors.is_empty(),
        "forwarder shutdown failed: {}",
        errors.join("; ")
    );
    Ok(())
}

async fn handle_rerun_message(
    message: LogMsg,
    accumulators: &mut HashMap<StoreId, RecordingAccumulator>,
    blueprint_accumulators: &mut HashMap<StoreId, BlueprintAccumulator>,
    queue: &Arc<Mutex<DurableQueue>>,
    queue_events: &Arc<QueueEvents>,
    limits: RerunIngestLimits,
    finish_superseded_recordings: bool,
) -> Result<()> {
    let store_id = message.store_id().clone();
    if finish_superseded_recordings
        && store_id.kind() == StoreKind::Recording
        && matches!(message, LogMsg::SetStoreInfo(_))
    {
        let mut superseded_accumulators =
            take_superseded_recording_accumulators(accumulators, &store_id);
        for accumulator in &mut superseded_accumulators {
            flush_accumulator(accumulator, queue, queue_events, limits.batch_bytes).await?;
        }
        let changed = queue
            .lock()
            .expect("durable queue mutex poisoned")
            .request_finish_superseded(
                store_id.application_id().as_str(),
                store_id.recording_id().as_str(),
            )?;
        if changed > 0 {
            info!(
                superseded_recordings = changed,
                retired_accumulators = superseded_accumulators.len(),
                "new producer recording generation requested durable completion of prior generations"
            );
            queue_events.work_available.notify_one();
        }
    }
    match store_id.kind() {
        StoreKind::Recording => {
            let accumulator = match accumulators.entry(store_id.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(RecordingAccumulator::new(store_id)?)
                }
            };
            if accumulator.boundary_before(&message, limits.maximum_batch_source_span)?
                != BatchBoundary::Continue
            {
                flush_accumulator(accumulator, queue, queue_events, limits.batch_bytes).await?;
            }
            if matches!(message, LogMsg::SetStoreInfo(_)) && accumulator.pending_len() > 0 {
                flush_accumulator(accumulator, queue, queue_events, limits.batch_bytes).await?;
            }
            accumulator.push(message)?;
            if accumulator.pending_len() >= limits.batch_messages {
                flush_accumulator(accumulator, queue, queue_events, limits.batch_bytes).await?;
            }
        }
        StoreKind::Blueprint => {
            let retained_bytes = blueprint_accumulators
                .values()
                .map(BlueprintAccumulator::retained_bytes)
                .fold(0_u64, u64::saturating_add);
            if (!blueprint_accumulators.contains_key(&store_id)
                && blueprint_accumulators.len() >= MAXIMUM_INCOMPLETE_BLUEPRINT_STORES)
                || retained_bytes.saturating_add(message.total_size_bytes())
                    > limits.blueprint_bytes
            {
                blueprint_accumulators.remove(&store_id);
                warn!(store_id = ?store_id, "rejecting Rerun Blueprint because incomplete stores exhausted their aggregate budget");
                return Ok(());
            }
            let accumulator = match blueprint_accumulators.entry(store_id.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let accumulator = match BlueprintAccumulator::new(
                        store_id.clone(),
                        limits.blueprint_bytes,
                        limits.blueprint_messages,
                    ) {
                        Ok(accumulator) => accumulator,
                        Err(error) => {
                            warn!(%error, store_id = ?store_id, "rejecting unauthorized Rerun Blueprint");
                            return Ok(());
                        }
                    };
                    entry.insert(accumulator)
                }
            };
            let complete = match accumulator.push(message) {
                Ok(complete) => complete,
                Err(error) => {
                    blueprint_accumulators.remove(&store_id);
                    warn!(%error, store_id = ?store_id, "rejecting malformed Rerun Blueprint");
                    return Ok(());
                }
            };
            if !complete {
                return Ok(());
            }
            let accumulator = blueprint_accumulators
                .remove(&store_id)
                .expect("completed Blueprint accumulator exists");
            let recording = match associated_recording(accumulator.store_id(), accumulators.keys())
            {
                Ok(recording) => recording.clone(),
                Err(error) => {
                    warn!(%error, store_id = ?store_id, "rejecting unassociated Rerun Blueprint");
                    return Ok(());
                }
            };
            let blueprint = match accumulator.finish() {
                Ok(blueprint) => blueprint,
                Err(error) => {
                    warn!(%error, store_id = ?store_id, "rejecting oversized Rerun Blueprint");
                    return Ok(());
                }
            };
            queue_events
                .volatile_blueprints
                .fetch_add(1, Ordering::Relaxed);
            loop {
                let capacity_available = queue_events.capacity_available.notified();
                let result = queue
                    .lock()
                    .expect("durable queue mutex poisoned")
                    .enqueue_blueprint(
                        recording.application_id().as_str(),
                        recording.recording_id().as_str(),
                        &blueprint,
                    );
                match result {
                    Ok(_) => {
                        queue_events
                            .volatile_blueprints
                            .fetch_sub(1, Ordering::Relaxed);
                        queue_events.work_available.notify_one();
                        break;
                    }
                    Err(error) if error.downcast_ref::<QueueFull>().is_some() => {
                        capacity_available.await;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    }
    Ok(())
}

fn take_superseded_recording_accumulators(
    accumulators: &mut HashMap<StoreId, RecordingAccumulator>,
    current: &StoreId,
) -> Vec<RecordingAccumulator> {
    let superseded = accumulators
        .keys()
        .filter(|candidate| {
            candidate.kind() == StoreKind::Recording
                && candidate.application_id() == current.application_id()
                && *candidate != current
        })
        .cloned()
        .collect::<Vec<_>>();
    superseded
        .into_iter()
        .filter_map(|store_id| accumulators.remove(&store_id))
        .collect()
}

#[cfg(unix)]
async fn shutdown_signal() -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result?,
        _ = terminate.recv() => {}
    }
    Ok(())
}

#[cfg(not(unix))]
async fn shutdown_signal() -> Result<()> {
    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn canonical_authority(url: &url::Url) -> Result<String> {
    let host = url.host().context("canonical gateway URL has no host")?;
    let host = match host {
        url::Host::Ipv6(address) => format!("[{address}]"),
        other => other.to_string(),
    };
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

async fn flush_accumulators(
    accumulators: &mut HashMap<StoreId, RecordingAccumulator>,
    queue: &Arc<Mutex<DurableQueue>>,
    queue_events: &Arc<QueueEvents>,
    maximum_batch_bytes: u64,
) -> Result<()> {
    for accumulator in accumulators.values_mut() {
        flush_accumulator(accumulator, queue, queue_events, maximum_batch_bytes).await?;
    }
    Ok(())
}

async fn flush_accumulator(
    accumulator: &mut RecordingAccumulator,
    queue: &Arc<Mutex<DurableQueue>>,
    queue_events: &Arc<QueueEvents>,
    maximum_batch_bytes: u64,
) -> Result<()> {
    let batches = accumulator.drain_encoded(maximum_batch_bytes)?;
    let application_id = accumulator.store_id().application_id().as_str().to_owned();
    let recording_id = accumulator.store_id().recording_id().as_str().to_owned();
    queue_events
        .volatile_batches
        .fetch_add(batches.len(), Ordering::Relaxed);
    for batch in batches {
        enqueue_batch(queue, queue_events, &application_id, &recording_id, &batch).await?;
        queue_events
            .volatile_batches
            .fetch_sub(1, Ordering::Relaxed);
    }
    Ok(())
}

async fn enqueue_batch(
    queue: &Mutex<DurableQueue>,
    events: &QueueEvents,
    application_id: &str,
    recording_id: &str,
    batch: &veoveo_recording_protocol::v1::RecordingBatch,
) -> Result<()> {
    loop {
        let capacity_available = events.capacity_available.notified();
        let result = queue.lock().expect("durable queue mutex poisoned").enqueue(
            application_id,
            recording_id,
            batch,
        );
        match result {
            Ok(_) => {
                events.work_available.notify_one();
                return Ok(());
            }
            Err(error) if error.downcast_ref::<QueueFull>().is_some() => capacity_available.await,
            Err(error) => return Err(error),
        }
    }
}

async fn upload_loop(
    queue: Arc<Mutex<DurableQueue>>,
    queue_events: Arc<QueueEvents>,
    client: RecordingIngestClient,
    stop: CancellationToken,
) -> Result<()> {
    let mut backoff = Duration::from_millis(250);
    let mut previous_diagnostics = None;
    loop {
        if stop.is_cancelled() {
            return Ok(());
        }
        let work_available = queue_events.work_available.notified();
        let pass = tokio::select! {
            _ = stop.cancelled() => return Ok(()),
            result = upload_pass(&queue, &queue_events, &client, false) => result,
        };
        match pass {
            Ok(progress) => {
                backoff = Duration::from_millis(250);
                log_queue_diagnostics(&queue, &mut previous_diagnostics)?;
                if !progress {
                    tokio::select! {
                        _ = stop.cancelled() => return Ok(()),
                        _ = work_available => {}
                    }
                }
            }
            Err(error) => {
                log_queue_diagnostics(&queue, &mut previous_diagnostics)?;
                warn!(error = ?error, retry_milliseconds = backoff.as_millis(), "recording upload deferred");
                tokio::select! {
                    _ = stop.cancelled() => return Ok(()),
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
    }
}

async fn drain_and_finish(
    queue: Arc<Mutex<DurableQueue>>,
    client: &RecordingIngestClient,
) -> Result<()> {
    let queue_events = QueueEvents::default();
    loop {
        upload_pass(&queue, &queue_events, client, true).await?;
        if queue
            .lock()
            .expect("durable queue mutex poisoned")
            .streams()?
            .is_empty()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn upload_pass(
    queue: &Arc<Mutex<DurableQueue>>,
    queue_events: &QueueEvents,
    client: &RecordingIngestClient,
    finish_empty: bool,
) -> Result<bool> {
    let streams = queue
        .lock()
        .expect("durable queue mutex poisoned")
        .streams()?;
    let mut progress = false;
    for mut stream in streams {
        'generation: loop {
            if stream.remote_stream_id.is_none() {
                let opened = client
                    .open(&OpenRecordingStreamRequest {
                        source_stream_id: stream.source_stream_id.clone(),
                        application_id: stream.application_id.clone(),
                        recording_id: stream.recording_id.clone(),
                    })
                    .await?;
                ensure!(
                    opened.next_sequence == 1,
                    "new recording ingest generation did not start at sequence one"
                );
                stream = queue
                    .lock()
                    .expect("durable queue mutex poisoned")
                    .mark_opened(&stream, &opened.stream_id)?;
                progress = true;
            }
            let batch = queue
                .lock()
                .expect("durable queue mutex poisoned")
                .next_batch(&stream)?;
            let Some(queued) = batch else { break };
            let mut remote_batch = queued.batch;
            remote_batch.sequence = queued
                .local_sequence
                .checked_sub(stream.remote_first_local_sequence)
                .and_then(|offset| offset.checked_add(1))
                .context("queued batch precedes its remote generation")?;
            let result = client
                .append(
                    stream
                        .remote_stream_id
                        .as_deref()
                        .context("queued stream has no remote identity")?,
                    &remote_batch,
                )
                .await;
            if let Err(error) = &result
                && let Some(finish_generation) = stream_rollover(error)
            {
                if finish_generation {
                    client
                        .finish(
                            stream
                                .remote_stream_id
                                .as_deref()
                                .context("queued stream has no remote identity")?,
                            RecordingStreamFinishMode::ContinueRecording,
                        )
                        .await?;
                }
                stream = queue
                    .lock()
                    .expect("durable queue mutex poisoned")
                    .rollover(&stream)?;
                progress = true;
                continue 'generation;
            }
            if let Err(error) = &result
                && let Some(ingest) = error.downcast_ref::<IngestRequestError>()
                && let Some(seconds) = ingest.retry_after_seconds
            {
                tokio::time::sleep(Duration::from_secs(seconds.min(60))).await;
            }
            let result = result?;
            ensure!(
                result.durable_through_sequence >= remote_batch.sequence,
                "gateway did not durably acknowledge the uploaded batch"
            );
            stream = queue
                .lock()
                .expect("durable queue mutex poisoned")
                .acknowledge(&stream, queued.local_sequence)?;
            queue_events.capacity_available.notify_waiters();
            progress = true;
        }
        loop {
            let queued = {
                queue
                    .lock()
                    .expect("durable queue mutex poisoned")
                    .next_blueprint(&stream)?
            };
            let Some(queued) = queued else {
                break;
            };
            let result = client
                .publish_blueprint(
                    stream
                        .remote_stream_id
                        .as_deref()
                        .context("queued stream has no remote identity")?,
                    &queued.blueprint,
                )
                .await;
            if let Err(error) = &result
                && blueprint_permanent_rejection(error)
            {
                tracing::error!(
                    revision = queued.revision,
                    %error,
                    "governed producer Blueprint was rejected; recording data remains active"
                );
                stream = queue
                    .lock()
                    .expect("durable queue mutex poisoned")
                    .acknowledge_blueprint(&stream, queued.revision)?;
                queue_events.capacity_available.notify_waiters();
                progress = true;
                continue;
            }
            let result = result?;
            ensure!(
                result.revision == queued.revision && result.sha256 == queued.blueprint.sha256,
                "gateway acknowledged a different Blueprint revision or digest"
            );
            stream = queue
                .lock()
                .expect("durable queue mutex poisoned")
                .acknowledge_blueprint(&stream, queued.revision)?;
            queue_events.capacity_available.notify_waiters();
            progress = true;
        }
        if (finish_empty || stream.finish_requested)
            && !queue
                .lock()
                .expect("durable queue mutex poisoned")
                .has_pending(&stream)?
        {
            client
                .finish(
                    stream
                        .remote_stream_id
                        .as_deref()
                        .context("queued stream has no remote identity")?,
                    RecordingStreamFinishMode::CompleteRecording,
                )
                .await?;
            queue
                .lock()
                .expect("durable queue mutex poisoned")
                .complete(&stream)?;
            progress = true;
        }
    }
    Ok(progress)
}

fn log_queue_diagnostics(
    queue: &Arc<Mutex<DurableQueue>>,
    previous: &mut Option<QueueDiagnostics>,
) -> Result<()> {
    let current = queue
        .lock()
        .expect("durable queue mutex poisoned")
        .diagnostics()?;
    if previous.as_ref() != Some(&current) {
        info!(
            queued_bytes = current.queued_bytes,
            maximum_bytes = current.maximum_bytes,
            streams = current.stream_count,
            open_streams = current.open_stream_count,
            pending_batches = current.pending_batch_count,
            pending_blueprints = current.pending_blueprint_count,
            finishing_streams = current.finishing_stream_count,
            "recording forwarder queue state changed"
        );
        *previous = Some(current);
    }
    Ok(())
}

fn stream_rollover(error: &anyhow::Error) -> Option<bool> {
    let ingest = error.downcast_ref::<IngestRequestError>()?;
    if ingest.code == IngestErrorCode::QuotaExceeded
        && ingest.quota == Some(RecordingIngestQuota::MaximumStreamBytes)
    {
        return Some(true);
    }
    (ingest.code == IngestErrorCode::StreamFinished).then_some(false)
}

fn blueprint_permanent_rejection(error: &anyhow::Error) -> bool {
    let Some(ingest) = error.downcast_ref::<IngestRequestError>() else {
        return false;
    };
    matches!(
        ingest.code,
        IngestErrorCode::BlueprintNotAllowed
            | IngestErrorCode::BlueprintAssociationMismatch
            | IngestErrorCode::InvalidBlueprint
            | IngestErrorCode::BlueprintRevisionConflict
    ) || (ingest.code == IngestErrorCode::QuotaExceeded
        && matches!(
            ingest.quota,
            Some(
                RecordingIngestQuota::MaximumBlueprintBytes
                    | RecordingIngestQuota::MaximumBlueprintMessages
                    | RecordingIngestQuota::MaximumBlueprintRevisions
            )
        ))
}

#[cfg(test)]
mod tests {
    use re_log_types::ApplicationId;
    use reqwest::StatusCode;

    use super::*;

    fn queued_batch() -> veoveo_recording_protocol::v1::RecordingBatch {
        use re_sdk::RecordingStreamBuilder;
        use re_sdk_types::archetypes::Scalars;
        let (recording, storage) = RecordingStreamBuilder::new("shutdown-camera")
            .recording_id("shutdown-run")
            .memory()
            .unwrap();
        recording.log("value", &Scalars::single(42.0)).unwrap();
        let messages = storage.take();
        let mut accumulator = RecordingAccumulator::new(messages[0].store_id().clone()).unwrap();
        for message in messages {
            accumulator.push(message).unwrap();
        }
        accumulator
            .drain_encoded(8 * 1024 * 1024)
            .unwrap()
            .remove(0)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shutdown_refuses_single_poll_late_completion_and_already_joined_jobs() -> Result<()> {
        for fail_work in [false, true] {
            let progress = Mutex::new(ShutdownProgress::default());
            let events = QueueEvents::default();
            let (outcome, end) = shutdown_work(
                async {
                    // Deliberately model one finite non-yielding encoding/fsync
                    // poll. This checks admission, not runtime preemption.
                    std::thread::sleep(Duration::from_millis(50));
                    ensure!(!fail_work, "retained work failure");
                    Ok(())
                },
                async { Ok(()) },
                Duration::from_millis(10),
                || {},
                &progress,
                &events,
            )
            .await;
            let error = format!("{:#}", outcome.unwrap_err());
            assert!(error.contains("completed after original deadline"));
            assert_eq!(error.contains("retained work failure"), fail_work);
            assert!(tokio::time::Instant::now() > end);

            let mut handle = tokio::spawn(async { Ok(()) });
            (&mut handle).await??;
            let error = RetiringJob {
                stop: CancellationToken::new(),
                handle: &mut handle,
                joined: true,
            }
            .join(end)
            .await
            .unwrap_err();
            assert!(error.to_string().contains("retirement deadline expired"));
        }
        Ok(())
    }

    #[tokio::test]
    async fn shutdown_signal_interrupts_full_queue_wait_and_retains_durable_restart() -> Result<()>
    {
        use prost::Message;
        let root = tempfile::tempdir()?;
        let batch = queued_batch();
        let bytes = batch.encoded_len() as u64;
        let queue = Mutex::new(DurableQueue::open(root.path().join("queue"), bytes)?);
        queue.lock().unwrap().enqueue("camera", "run", &batch)?;
        let before = queue.lock().unwrap().streams()?;
        let events = QueueEvents::default();
        events.volatile_batches.store(1, Ordering::Relaxed);
        let progress = Mutex::new(ShutdownProgress {
            stage: ShutdownStage::Flush,
            burst_messages: 1,
            ..Default::default()
        });
        let stopped = std::sync::atomic::AtomicBool::new(false);
        let start = tokio::time::Instant::now();
        let (result, end) = shutdown_work(
            enqueue_batch(&queue, &events, "camera", "run", &batch),
            async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(())
            },
            Duration::from_millis(100),
            || stopped.store(true, Ordering::Relaxed),
            &progress,
            &events,
        )
        .await;
        let error = result.unwrap_err().to_string();
        assert!(stopped.load(Ordering::Relaxed));
        assert!(error.contains("stage=Flush") && error.contains("volatile_encoded_batches=1"));
        assert!(start.elapsed() < Duration::from_millis(300));
        assert!(tokio::time::Instant::now() < end);
        drop(queue);
        let reopened = DurableQueue::open(root.path().join("queue"), bytes)?;
        assert_eq!(before, reopened.streams()?);
        assert_eq!(reopened.diagnostics()?.pending_batch_count, 1);
        Ok(())
    }

    #[tokio::test]
    async fn shutdown_deadline_includes_channel_drain_and_joins_original_receiver() -> Result<()> {
        let (source, receiver) = re_log_channel::log_channel(re_log_channel::LogSource::Sdk);
        let (tx, mut rx) = mpsc::channel(1);
        let stop = CancellationToken::new();
        let mut job = spawn_receiver(receiver, tx, stop.clone(), Arc::new(QueueEvents::default()));
        let events = QueueEvents::default();
        let progress = Mutex::new(ShutdownProgress::default());
        let (outcome, end) = shutdown_work(
            async {
                while rx.recv().await.is_some() {}
                Ok(())
            },
            async { Ok(()) },
            Duration::from_millis(300),
            || {},
            &progress,
            &events,
        )
        .await;
        assert!(outcome.is_err());
        let uploader_stop = CancellationToken::new();
        let task_stop = uploader_stop.clone();
        let mut uploader = tokio::spawn(async move {
            task_stop.cancelled().await;
            Ok(())
        });
        assert!(
            retire_jobs(
                outcome,
                &mut rx,
                RetiringJob {
                    stop,
                    handle: &mut job,
                    joined: false
                },
                RetiringJob {
                    stop: uploader_stop,
                    handle: &mut uploader,
                    joined: false
                },
                end
            )
            .await
            .is_err()
        );
        assert!(job.is_finished() && uploader.is_finished());
        assert!(tokio::time::Instant::now() <= end);
        drop(source);
        Ok(())
    }

    #[tokio::test]
    async fn forced_retirement_releases_actual_blocking_send_and_joins_worker() -> Result<()> {
        let (source, receiver) = re_log_channel::log_channel(re_log_channel::LogSource::Sdk);
        let (tx, mut rx) = mpsc::channel(1);
        tx.send(Vec::new()).await?;
        let (recording, storage) = re_sdk::RecordingStreamBuilder::new("blocked-camera")
            .recording_id("blocked-run")
            .memory()?;
        recording.log("value", &re_sdk_types::archetypes::Scalars::single(42.0))?;
        for message in storage.take() {
            source.send(DataSourceMessage::LogMsg(message))?;
        }
        let stop = CancellationToken::new();
        let events = Arc::new(QueueEvents::default());
        let mut job = spawn_receiver(receiver, tx, stop.clone(), events.clone());
        tokio::time::timeout(Duration::from_secs(1), async {
            while events.receiver_buffered_messages.load(Ordering::Relaxed) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(!job.is_finished());
        let uploader_stop = CancellationToken::new();
        let mut uploader = tokio::spawn(async { Ok(()) });
        retire_jobs(
            Err(anyhow::anyhow!("fixture failed drain")),
            &mut rx,
            RetiringJob {
                stop,
                handle: &mut job,
                joined: false,
            },
            RetiringJob {
                stop: uploader_stop,
                handle: &mut uploader,
                joined: false,
            },
            tokio::time::Instant::now() + Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert!(job.is_finished() && uploader.is_finished());
        assert!(events.receiver_buffered_messages.load(Ordering::Relaxed) > 0);
        drop(source);
        Ok(())
    }

    #[tokio::test]
    async fn healthy_shutdown_drains_and_joins_full_receiver_channel() -> Result<()> {
        let (source, receiver) = re_log_channel::log_channel(re_log_channel::LogSource::Sdk);
        let (tx, mut rx) = mpsc::channel(1);
        // Occupy capacity, proving retirement releases an actual blocking_send.
        tx.send(Vec::new()).await?;
        let (recording, storage) = re_sdk::RecordingStreamBuilder::new("shutdown-camera")
            .recording_id("shutdown-run")
            .memory()?;
        recording.log("value", &re_sdk_types::archetypes::Scalars::single(42.0))?;
        for message in storage.take() {
            source.send(DataSourceMessage::LogMsg(message))?;
        }
        drop(source);
        let stop = CancellationToken::new();
        let mut job = spawn_receiver(receiver, tx, stop.clone(), Arc::new(QueueEvents::default()));
        let progress = Mutex::new(ShutdownProgress::default());
        let events = QueueEvents::default();
        let mut count = 0;
        let (result, end) = shutdown_work(
            async {
                while let Some(burst) = rx.recv().await {
                    count += burst.len();
                }
                Ok(())
            },
            async { Ok(()) },
            Duration::from_secs(1),
            || {},
            &progress,
            &events,
        )
        .await;
        result?;
        assert!(count >= 2);
        let uploader_stop = CancellationToken::new();
        let mut uploader = tokio::spawn(async { Ok(()) });
        retire_jobs(
            Ok(()),
            &mut rx,
            RetiringJob {
                stop,
                handle: &mut job,
                joined: false,
            },
            RetiringJob {
                stop: uploader_stop,
                handle: &mut uploader,
                joined: false,
            },
            end,
        )
        .await?;
        assert!(job.is_finished() && uploader.is_finished());
        Ok(())
    }

    fn ingest_error(code: IngestErrorCode, quota: Option<RecordingIngestQuota>) -> anyhow::Error {
        IngestRequestError {
            status: StatusCode::TOO_MANY_REQUESTS,
            code,
            quota,
            message: "ingest rejected the batch".to_owned(),
            retry_after_seconds: None,
        }
        .into()
    }

    #[test]
    fn superseded_generation_is_retired_before_blueprint_association() {
        let old = StoreId::recording("inspection-camera", "old");
        let current = StoreId::recording("inspection-camera", "current");
        let unrelated = StoreId::recording("other-camera", "current");
        let mut accumulators = HashMap::from([
            (old.clone(), RecordingAccumulator::new(old.clone()).unwrap()),
            (
                unrelated.clone(),
                RecordingAccumulator::new(unrelated.clone()).unwrap(),
            ),
        ]);

        let retired = take_superseded_recording_accumulators(&mut accumulators, &current);
        assert_eq!(retired.len(), 1);
        assert_eq!(retired[0].store_id(), &old);
        assert!(accumulators.contains_key(&unrelated));
        accumulators.insert(
            current.clone(),
            RecordingAccumulator::new(current.clone()).unwrap(),
        );

        let blueprint = StoreId::default_blueprint(ApplicationId::from("inspection-camera"));
        assert_eq!(
            crate::blueprint::associated_recording(&blueprint, accumulators.keys()).unwrap(),
            &current
        );
    }

    #[test]
    fn rolls_over_only_permanent_stream_boundaries() {
        assert_eq!(
            stream_rollover(&ingest_error(
                IngestErrorCode::QuotaExceeded,
                Some(RecordingIngestQuota::MaximumStreamBytes),
            )),
            Some(true)
        );
        assert_eq!(
            stream_rollover(&ingest_error(IngestErrorCode::StreamFinished, None)),
            Some(false)
        );
        assert_eq!(
            stream_rollover(&ingest_error(
                IngestErrorCode::QuotaExceeded,
                Some(RecordingIngestQuota::MaximumBytesPerDay),
            )),
            None
        );
    }

    #[test]
    fn blueprint_rejections_do_not_terminate_recording_ingest() {
        for code in [
            IngestErrorCode::BlueprintNotAllowed,
            IngestErrorCode::BlueprintAssociationMismatch,
            IngestErrorCode::InvalidBlueprint,
            IngestErrorCode::BlueprintRevisionConflict,
        ] {
            assert!(blueprint_permanent_rejection(&ingest_error(code, None)));
        }
        for quota in [
            RecordingIngestQuota::MaximumBlueprintBytes,
            RecordingIngestQuota::MaximumBlueprintMessages,
            RecordingIngestQuota::MaximumBlueprintRevisions,
        ] {
            assert!(blueprint_permanent_rejection(&ingest_error(
                IngestErrorCode::QuotaExceeded,
                Some(quota),
            )));
        }
        assert!(!blueprint_permanent_rejection(&ingest_error(
            IngestErrorCode::QuotaExceeded,
            Some(RecordingIngestQuota::MaximumBytesPerDay),
        )));
    }
}
