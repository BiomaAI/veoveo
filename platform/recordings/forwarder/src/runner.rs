use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use re_grpc_server::{MemoryLimit, ServerOptions, shutdown};
use re_log_channel::{DataSourceMessage, RecvTimeoutError};
use re_log_types::{LogMsg, StoreId, StoreKind};
use reqwest::header::{HOST, HeaderMap, HeaderValue};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use veoveo_recording_protocol::v1::OpenRecordingStreamRequest;

use crate::{
    batch::RecordingAccumulator,
    client::{IngestRequestError, RecordingIngestClient},
    config::ForwarderConfig,
    oauth::OAuthTokenProvider,
    queue::{DurableQueue, QueueFull},
};

pub async fn run(config: ForwarderConfig) -> Result<()> {
    config.validate()?;
    let _ = rustls::crypto::ring::default_provider().install_default();
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
            OAuthTokenProvider::new(
                http,
                token_endpoint,
                token_transport_endpoint,
                protected_resource,
                client_id,
                key_id,
                algorithm,
                &private_key_pem_file,
            )
        },
    )
    .await?;
    ensure!(
        config.maximum_queue_bytes >= client.maximum_batch_bytes(),
        "durable queue must hold at least one maximum-size gateway batch"
    );
    let queue = Arc::new(Mutex::new(DurableQueue::open(
        config.queue_dir.clone(),
        config.maximum_queue_bytes,
    )?));
    let uploader_stop = CancellationToken::new();
    let uploader = tokio::spawn(upload_loop(
        queue.clone(),
        client.clone(),
        uploader_stop.child_token(),
    ));

    let (grpc_stop_signal, grpc_shutdown) = shutdown::shutdown();
    let (receiver, _grpc_handle) = re_grpc_server::spawn_with_recv(
        config.bind,
        ServerOptions {
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
                flush_accumulators(&mut accumulators, &queue, client.maximum_batch_bytes()).await?;
            }
            message = message_rx.recv() => {
                let Some(message) = message else { break; };
                let store_id = message.store_id().clone();
                if store_id.kind() != StoreKind::Recording {
                    warn!(store_id = ?store_id, "ignoring non-recording Rerun store");
                    continue;
                }
                let accumulator = match accumulators.entry(store_id.clone()) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(RecordingAccumulator::new(store_id)?)
                    }
                };
                if matches!(message, LogMsg::SetStoreInfo(_)) && accumulator.pending_len() > 0 {
                    flush_accumulator(accumulator, &queue, client.maximum_batch_bytes()).await?;
                }
                accumulator.push(message)?;
                if accumulator.pending_len() >= config.batch_message_limit {
                    flush_accumulator(accumulator, &queue, client.maximum_batch_bytes()).await?;
                }
            }
        }
    }

    grpc_stop_signal.stop();
    receiver_stop.store(true, Ordering::SeqCst);
    receiver_task
        .await
        .context("Rerun receiver task panicked")??;
    while let Ok(message) = message_rx.try_recv() {
        let store_id = message.store_id().clone();
        if store_id.kind() != StoreKind::Recording {
            continue;
        }
        let accumulator = accumulators
            .entry(store_id.clone())
            .or_insert(RecordingAccumulator::new(store_id)?);
        if matches!(message, LogMsg::SetStoreInfo(_)) && accumulator.pending_len() > 0 {
            flush_accumulator(accumulator, &queue, client.maximum_batch_bytes()).await?;
        }
        accumulator.push(message)?;
    }
    flush_accumulators(&mut accumulators, &queue, client.maximum_batch_bytes()).await?;
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
    maximum_batch_bytes: u64,
) -> Result<()> {
    for accumulator in accumulators.values_mut() {
        flush_accumulator(accumulator, queue, maximum_batch_bytes).await?;
    }
    Ok(())
}

async fn flush_accumulator(
    accumulator: &mut RecordingAccumulator,
    queue: &Arc<Mutex<DurableQueue>>,
    maximum_batch_bytes: u64,
) -> Result<()> {
    let batches = accumulator.drain_encoded(maximum_batch_bytes)?;
    let application_id = accumulator.store_id().application_id().as_str().to_owned();
    let recording_id = accumulator.store_id().recording_id().as_str().to_owned();
    for batch in batches {
        loop {
            let result = queue.lock().expect("durable queue mutex poisoned").enqueue(
                &application_id,
                &recording_id,
                &batch,
            );
            match result {
                Ok(_) => break,
                Err(error) if error.downcast_ref::<QueueFull>().is_some() => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

async fn upload_loop(
    queue: Arc<Mutex<DurableQueue>>,
    client: RecordingIngestClient,
    stop: CancellationToken,
) -> Result<()> {
    let mut backoff = Duration::from_millis(250);
    loop {
        if stop.is_cancelled() {
            return Ok(());
        }
        match upload_pass(&queue, &client, false).await {
            Ok(progress) => {
                backoff = Duration::from_millis(250);
                if !progress {
                    tokio::select! {
                        _ = stop.cancelled() => return Ok(()),
                        _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                    }
                }
            }
            Err(error) => {
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
    loop {
        upload_pass(&queue, client, true).await?;
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
    client: &RecordingIngestClient,
    finish_empty: bool,
) -> Result<bool> {
    let streams = queue
        .lock()
        .expect("durable queue mutex poisoned")
        .streams()?;
    let mut progress = false;
    for mut stream in streams {
        if stream.remote_stream_id.is_none() {
            let opened = client
                .open(&OpenRecordingStreamRequest {
                    source_stream_id: stream.source_stream_id.clone(),
                    application_id: stream.application_id.clone(),
                    recording_id: stream.recording_id.clone(),
                })
                .await?;
            queue
                .lock()
                .expect("durable queue mutex poisoned")
                .mark_opened(&stream, &opened.stream_id)?;
            stream.remote_stream_id = Some(opened.stream_id);
            progress = true;
        }
        let batches = queue
            .lock()
            .expect("durable queue mutex poisoned")
            .batches(&stream)?;
        for batch in batches {
            let result = client
                .append(
                    stream
                        .remote_stream_id
                        .as_deref()
                        .context("queued stream has no remote identity")?,
                    &batch,
                )
                .await;
            if let Err(error) = &result
                && let Some(ingest) = error.downcast_ref::<IngestRequestError>()
                && let Some(seconds) = ingest.retry_after_seconds
            {
                tokio::time::sleep(Duration::from_secs(seconds.min(60))).await;
            }
            let result = result?;
            ensure!(
                result.durable_through_sequence >= batch.sequence,
                "gateway did not durably acknowledge the uploaded batch"
            );
            queue
                .lock()
                .expect("durable queue mutex poisoned")
                .acknowledge(&stream, batch.sequence)?;
            progress = true;
        }
        if finish_empty
            && queue
                .lock()
                .expect("durable queue mutex poisoned")
                .batches(&stream)?
                .is_empty()
        {
            client
                .finish(stream.remote_stream_id.as_deref().unwrap())
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
