use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use re_byte_size::SizeBytes as _;
use re_grpc_server::{MemoryLimit, PlaybackBehavior, ServerOptions, shutdown};
use re_log_channel::{DataSourceMessage, RecvTimeoutError};
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
}

#[derive(Debug, Default)]
struct QueueEvents {
    work_available: Notify,
    capacity_available: Notify,
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
    };
    let queue = Arc::new(Mutex::new(DurableQueue::open(
        config.queue_dir.clone(),
        config.maximum_queue_bytes,
    )?));
    let queue_events = Arc::new(QueueEvents::default());
    let uploader_stop = CancellationToken::new();
    let uploader = tokio::spawn(upload_loop(
        queue.clone(),
        queue_events.clone(),
        client.clone(),
        uploader_stop.child_token(),
    ));

    let (grpc_stop_signal, grpc_shutdown) = shutdown::shutdown();
    let (receiver, _grpc_handle) = re_grpc_server::spawn_with_recv(
        config.bind,
        ServerOptions {
            playback_behavior: PlaybackBehavior::NewestFirst,
            memory_limit: MemoryLimit::from_bytes(config.grpc_memory_limit_bytes),
            ..Default::default()
        },
        grpc_shutdown,
    );
    info!(bind = %config.bind, "recording forwarder loopback Rerun receiver up");
    let (message_tx, mut message_rx) = mpsc::channel::<LogMsg>(1024);
    let receiver_stop = Arc::new(AtomicBool::new(false));
    let blocking_stop = receiver_stop.clone();
    let receiver_task = tokio::task::spawn_blocking(move || -> Result<()> {
        while !blocking_stop.load(Ordering::SeqCst) {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(message) => {
                    if let Some(DataSourceMessage::LogMsg(message)) = message.into_data()
                        && message_tx.blocking_send(message).is_err()
                    {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(())
    });

    let mut accumulators = HashMap::<StoreId, RecordingAccumulator>::new();
    let mut blueprint_accumulators = HashMap::<StoreId, BlueprintAccumulator>::new();
    let mut flush = tokio::time::interval(config.flush_interval());
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            signal = &mut shutdown => {
                signal?;
                break;
            }
            _ = flush.tick() => {
                flush_accumulators(&mut accumulators, &queue, &queue_events, client.maximum_batch_bytes()).await?;
            }
            message = message_rx.recv() => {
                let Some(message) = message else { break; };
                handle_rerun_message(
                    message,
                    &mut accumulators,
                    &mut blueprint_accumulators,
                    &queue,
                    &queue_events,
                    limits,
                )
                .await?;
            }
        }
    }

    grpc_stop_signal.stop();
    receiver_stop.store(true, Ordering::SeqCst);
    receiver_task
        .await
        .context("Rerun receiver task panicked")??;
    while let Ok(message) = message_rx.try_recv() {
        handle_rerun_message(
            message,
            &mut accumulators,
            &mut blueprint_accumulators,
            &queue,
            &queue_events,
            limits,
        )
        .await?;
    }
    for (store_id, _) in blueprint_accumulators.drain() {
        warn!(store_id = ?store_id, "discarding incomplete Rerun Blueprint at shutdown");
    }
    flush_accumulators(
        &mut accumulators,
        &queue,
        &queue_events,
        client.maximum_batch_bytes(),
    )
    .await?;
    queue
        .lock()
        .expect("durable queue mutex poisoned")
        .request_finish_all()?;
    queue_events.work_available.notify_one();
    uploader_stop.cancel();
    uploader
        .await
        .context("recording uploader task panicked")??;
    let drained = tokio::time::timeout(
        config.shutdown_drain_window(),
        drain_and_finish(queue.clone(), &client),
    )
    .await;
    if !matches!(drained, Ok(Ok(()))) {
        warn!("shutdown drain did not complete; durable batches remain queued for restart");
    }
    Ok(())
}

async fn handle_rerun_message(
    message: LogMsg,
    accumulators: &mut HashMap<StoreId, RecordingAccumulator>,
    blueprint_accumulators: &mut HashMap<StoreId, BlueprintAccumulator>,
    queue: &Arc<Mutex<DurableQueue>>,
    queue_events: &Arc<QueueEvents>,
    limits: RerunIngestLimits,
) -> Result<()> {
    let store_id = message.store_id().clone();
    match store_id.kind() {
        StoreKind::Recording => {
            let accumulator = match accumulators.entry(store_id.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(RecordingAccumulator::new(store_id)?)
                }
            };
            if accumulator.boundary_before(&message)? == BatchBoundary::StartVideoGop {
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
    for batch in batches {
        loop {
            let capacity_available = queue_events.capacity_available.notified();
            let result = queue.lock().expect("durable queue mutex poisoned").enqueue(
                &application_id,
                &recording_id,
                &batch,
            );
            match result {
                Ok(_) => {
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
    Ok(())
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
        match upload_pass(&queue, &queue_events, &client, false).await {
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
                warn!(%error, retry_milliseconds = backoff.as_millis(), "recording upload deferred");
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
    use reqwest::StatusCode;

    use super::*;

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
