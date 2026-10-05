use super::{DurableRequest, SpeechService};
use crate::worker::{WorkerConnection, WorkerEvent, WorkerRequest};
use anyhow::{Context, Result, bail, ensure};
use futures::{StreamExt, TryStreamExt};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{ArtifactReadAuthority, ArtifactTaskId};
use veoveo_speech_contract::transcript::{MAX_RECORDING_SECONDS, Transcript};
use veoveo_speech_contract::{MAX_SOURCE_BYTES, TranscriptionId, validate_source};
use veoveo_task_runtime::{TaskFailure, TaskSnapshot, TaskStatus, TaskTransition};

const LEASE: Duration = Duration::from_secs(60);

impl SpeechService {
    pub(super) async fn schedule(
        self: &Arc<Self>,
        snapshot: TaskSnapshot,
        request: DurableRequest,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<TaskSnapshot> {
        let task = TranscriptionId::try_from(snapshot.task_id)?;
        let claimed = self.tasks.claim(task.task_id(), LEASE).await?;
        let cancel = CancellationToken::new();
        let worker = {
            let service = self.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                let _permit = permit;
                service.run(task, request, cancel).await;
            })
        };
        self.tasks
            .register_worker(task.task_id(), cancel, worker)
            .await?;
        Ok(claimed.snapshot)
    }

    async fn run(
        self: Arc<Self>,
        task: TranscriptionId,
        request: DurableRequest,
        cancel: CancellationToken,
    ) {
        let task_ids = [task.task_id()];
        // Subscription establishment participates in the same select as lease
        // renewal and cancellation; a slow baseline cannot strand a live worker.
        let mut updates =
            Box::pin(futures::stream::once(self.tasks.live_updates_for(&task_ids)).try_flatten());
        let mut work = Box::pin(async {
            tokio::time::timeout(Duration::from_secs(900), self.execute(task, request)).await?
        });
        let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
        heartbeat.tick().await;
        let transition = loop {
            tokio::select! {
                result = &mut work => break match result {
                    Ok(result) => veoveo_task_runtime::mcp_task_completion("Transcript ready", result)
                        .unwrap_or_else(|_| TaskTransition::Failed(TaskFailure::new("result_admission_failed",
                            "Transcript completion could not be admitted."))),
                    Err(error) => {
                        // No provider exception or captured text belongs in public errors.
                        tracing::warn!(%task, error_type = %error.root_cause(), "speech transcription failed");
                        TaskTransition::Failed(TaskFailure::new("transcription_failed",
                            "Transcription could not finish. Check source access, audio format and Speech service availability."))
                    }
                },
                () = cancel.cancelled() => break TaskTransition::Cancelled,
                update = updates.next() => {
                    match update {
                        Some(Ok(update)) if update.snapshot.status == TaskStatus::CancelRequested => break TaskTransition::Cancelled,
                        Some(Ok(_)) => (),
                        _ => return,
                    }
                },
                _ = heartbeat.tick() => {
                    match self.tasks.renew_lease(task.task_id(), LEASE).await {
                        Ok(snapshot) if snapshot.status == TaskStatus::CancelRequested => break TaskTransition::Cancelled,
                        Ok(_) => (),
                        Err(_) => return,
                    }
                }
            }
        };
        // Disconnect inference and remove its private source before terminal acknowledgement.
        drop(work);
        let transition = if self
            .tasks
            .is_cancel_requested(task.task_id())
            .await
            .unwrap_or(true)
        {
            TaskTransition::Cancelled
        } else {
            transition
        };
        if let Err(error) = self.tasks.transition(task.task_id(), transition).await {
            tracing::warn!(%task, %error, "speech Task settlement lost its lease");
        }
    }

    async fn progress(&self, task: TranscriptionId, message: &str, progress: f64) -> Result<()> {
        self.tasks
            .transition(
                task.task_id(),
                TaskTransition::Running {
                    message: message.into(),
                    progress,
                },
            )
            .await?;
        Ok(())
    }

    async fn execute(
        &self,
        task: TranscriptionId,
        request: DurableRequest,
    ) -> Result<rmcp::model::CallToolResult> {
        let _slot = self.slots.acquire().await?;
        self.progress(task, "Reading authorized recording", 0.0)
            .await?;
        let work = tempfile::Builder::new()
            .prefix("recording-")
            .tempdir_in(self.worker.workspace())?;
        let path = work.path().join("source");
        let authority = ArtifactReadAuthority::Task {
            capability: &request.read,
            task_id: ArtifactTaskId::parse(task.to_string())?,
        };
        let download = self
            .artifacts
            .download_with_authority(authority, request.source.artifact_id())
            .await?;
        validate_source(&download.metadata)?;
        ensure!(
            download.metadata.artifact_id() == request.source.artifact_id()
                && download.metadata.byte_len == request.source.byte_len,
            "source identity changed"
        );
        let source = download.metadata;
        let mut stream = download.response.bytes_stream();
        let mut file = tokio::fs::File::create(&path).await?;
        let mut size = 0u64;
        let mut hash = Sha256::new();
        while let Some(chunk) = tokio::time::timeout(Duration::from_secs(30), stream.next()).await?
        {
            let chunk = chunk?;
            size = size
                .checked_add(chunk.len() as u64)
                .context("source size overflow")?;
            ensure!(
                size <= source.byte_len && size <= MAX_SOURCE_BYTES,
                "source exceeded admitted size"
            );
            hash.update(&chunk);
            file.write_all(&chunk).await?;
        }
        ensure!(
            size == source.byte_len,
            "source ended before its declared size"
        );
        file.flush().await?;
        drop(file);
        self.progress(task, "Transcribing recording", 0.0).await?;
        let mut inference = WorkerConnection::connect(
            self.worker.socket(),
            &WorkerRequest::File {
                path,
                max_duration_seconds: MAX_RECORDING_SECONDS,
            },
        )
        .await?;
        let transcript: Transcript = loop {
            match tokio::time::timeout(Duration::from_secs(60), inference.event()).await?? {
                WorkerEvent::Accepted => (),
                WorkerEvent::Transcript {
                    complete,
                    transcript,
                } => {
                    transcript.validate(MAX_RECORDING_SECONDS)?;
                    if complete {
                        break transcript;
                    }
                    let seconds = transcript
                        .segments
                        .last()
                        .map_or(0.0, |segment| segment.end);
                    let progress = if transcript.duration_seconds > 0.0 {
                        (seconds / transcript.duration_seconds).clamp(0.0, 0.99)
                    } else {
                        0.0
                    };
                    self.progress(task, "Transcribing recording", progress)
                        .await?;
                }
                WorkerEvent::Error { code } => bail!("speech worker {code:?}"),
                WorkerEvent::Ready { .. } => bail!("unexpected worker readiness"),
            }
        };
        self.progress(task, "Publishing transcript", 0.99).await?;
        // The read capability rechecks current source access before output publication.
        let current = self
            .artifacts
            .read_metadata(authority, source.artifact_id())
            .await?;
        self.tasks.renew_lease(task.task_id(), LEASE).await?;
        self.publish(
            task,
            &request.write,
            current,
            hex::encode(hash.finalize()),
            transcript,
        )
        .await
    }
}
