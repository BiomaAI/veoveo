//! Private public-OAuth upload intents and owned session cancellation.
use super::*;
use std::{
    future::Future,
    io::Write,
    pin::Pin,
    sync::{Arc, Mutex},
};
use veoveo_artifact_contract::ArtifactUploadId;
use veoveo_testing_support::lifecycle::owner::{self, CleanupKind, CleanupRegistration};

#[derive(Clone)]
pub(super) struct Journal(Arc<Mutex<fs::File>>);
#[derive(Serialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum Record<'a> {
    Prepared {
        schema: &'static str,
        profile: &'static str,
    },
    CreateIntent {
        key: &'a str,
        descriptor: &'a CreateArtifactUpload,
    },
    Acknowledged {
        upload_id: ArtifactUploadId,
    },
    MutationIntent {
        upload_id: ArtifactUploadId,
        operation: &'static str,
    },
    Received {
        receipt: &'a ArtifactUploadReceipt,
    },
    Published {
        receipt: &'a ArtifactUploadReceipt,
    },
    Cleanup {
        upload_id: Option<ArtifactUploadId>,
        status: CleanupStatus,
    },
    ConsumersPassed,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum CleanupStatus {
    Cancelled,
    RetainedPublished,
    Failed,
    UnresolvedCreation,
}
enum Settlement {
    Cancelled,
    Observed(Box<ArtifactUploadSession>),
}
type Close = Pin<Box<dyn Future<Output = Result<Settlement>> + Send>>;
struct State {
    future: Option<Close>,
    failed: bool,
    done: bool,
    deadline: Option<tokio::time::Instant>,
}
struct Cleanup {
    state: tokio::sync::Mutex<State>,
    id: Mutex<Option<ArtifactUploadId>>,
    observations: Mutex<Vec<ArtifactUploadReceipt>>,
    publication: Mutex<Option<ArtifactUploadReceipt>>,
    client: reqwest::Client,
    token: String,
    base: String,
    journal: Journal,
}
impl Journal {
    pub(super) fn create(output: &Path, focused: bool) -> Result<Self> {
        let path = output.with_extension("uploads.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let result = Self(Arc::new(Mutex::new(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?,
        )));
        result.append(&Record::Prepared {
            schema: "veoveo.ai/artifact-public-upload-intents/v1",
            profile: if focused { "focused" } else { "browser" },
        })?;
        Ok(result)
    }
    fn append(&self, record: &impl Serialize) -> Result<()> {
        let mut file = self.0.lock().expect("upload journal");
        file.write_all(&serde_json::to_vec(record)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }
    pub(super) fn passed(&self) -> Result<()> {
        self.append(&Record::ConsumersPassed)
    }
    pub(super) fn begin(
        &self,
        client: &reqwest::Client,
        token: &str,
        base: &str,
        key: &str,
        descriptor: &CreateArtifactUpload,
    ) -> Result<Upload> {
        self.append(&Record::CreateIntent { key, descriptor })?;
        let cleanup = Arc::new(Cleanup {
            state: tokio::sync::Mutex::new(State {
                future: None,
                failed: false,
                done: false,
                deadline: None,
            }),
            id: Mutex::new(None),
            observations: Mutex::new(Vec::new()),
            publication: Mutex::new(None),
            client: client.clone(),
            token: token.to_owned(),
            base: base.into(),
            journal: self.clone(),
        });
        let retained = cleanup.clone();
        let registration = owner::register_cleanup(
            CleanupKind::Remote,
            "artifact-public-upload",
            key,
            move || async move { retained.finish().await },
        )?;
        Ok(Upload {
            cleanup,
            registration,
        })
    }
}
pub(super) struct Upload {
    cleanup: Arc<Cleanup>,
    registration: CleanupRegistration,
}
impl Upload {
    pub(super) fn acknowledge(&self, id: ArtifactUploadId) -> Result<()> {
        *self.cleanup.id.lock().expect("upload identity") = Some(id);
        self.registration.observed_identity(&id.to_string())?;
        self.cleanup
            .journal
            .append(&Record::Acknowledged { upload_id: id })
    }
    pub(super) fn intent(&self, id: ArtifactUploadId, operation: &'static str) -> Result<()> {
        self.cleanup.journal.append(&Record::MutationIntent {
            upload_id: id,
            operation,
        })
    }
    // Receiving a typed receipt is an observation, before any independent
    // identity/content assertion. Failed sync retains it without settlement.
    pub(super) fn received(&self, receipt: &ArtifactUploadReceipt) -> Result<()> {
        let mut state = self
            .cleanup
            .state
            .try_lock()
            .context("receipt observation overlaps owned cleanup")?;
        let recorded = self.cleanup.record_observation(receipt);
        state.failed |= recorded.is_err();
        recorded
    }
    // The validated receipt is retained and synced synchronously, before the
    // caller can be cancelled at its next replay/read await.
    pub(super) fn published(&self, receipt: &ArtifactUploadReceipt) -> Result<()> {
        let mut state = self
            .cleanup
            .state
            .try_lock()
            .context("publication overlaps owned cleanup")?;
        ensure!(!state.failed, "public upload cleanup previously failed");
        let recorded = self
            .cleanup
            .record_publication(receipt)
            .and_then(|()| {
                self.cleanup.journal.append(&Record::Cleanup {
                    upload_id: Some(receipt.upload_id),
                    status: CleanupStatus::RetainedPublished,
                })
            })
            .and_then(|()| self.registration.settled());
        state.failed = recorded.is_err();
        state.done = !state.failed;
        recorded
    }
    pub(super) async fn cancel(&self) -> Result<()> {
        self.cleanup.finish().await?;
        self.registration.settled()
    }
}
impl Cleanup {
    fn record_observation(&self, receipt: &ArtifactUploadReceipt) -> Result<()> {
        let mut observed = self.observations.lock().expect("received receipts");
        if !observed.contains(receipt) {
            // The operation can receive one completion and one cleanup GET.
            ensure!(
                observed.len() < 2,
                "too many distinct received upload receipts"
            );
            observed.push(receipt.clone());
        }
        self.journal.append(&Record::Received { receipt })
    }
    fn record_publication(&self, receipt: &ArtifactUploadReceipt) -> Result<()> {
        ensure!(
            Some(receipt.upload_id) == *self.id.lock().expect("upload identity"),
            "publication changed acknowledged upload identity"
        );
        ensure!(
            self.observations
                .lock()
                .expect("received receipts")
                .contains(receipt),
            "publication has no retained received receipt"
        );
        let mut retained = self.publication.lock().expect("publication receipt");
        if let Some(first) = retained.as_ref() {
            ensure!(first == receipt, "publication changed retained occurrence");
        } else {
            *retained = Some(receipt.clone());
        }
        self.journal.append(&Record::Published { receipt })
    }
    async fn finish(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        ensure!(!state.failed, "public upload cleanup previously failed");
        if state.done {
            return Ok(());
        }
        let Some(id) = *self.id.lock().expect("upload identity") else {
            state.failed = true;
            self.journal.append(&Record::Cleanup {
                upload_id: None,
                status: CleanupStatus::UnresolvedCreation,
            })?;
            anyhow::bail!("public upload creation outcome unresolved");
        };
        if state.future.is_none() {
            let client = self.client.clone();
            let token = self.token.clone();
            let url = format!("{}/{id}", self.base);
            state.future = Some(Box::pin(async move {
                let status = client
                    .delete(&url)
                    .bearer_auth(&token)
                    .send()
                    .await?
                    .status();
                if status == StatusCode::NO_CONTENT {
                    return Ok(Settlement::Cancelled);
                }
                ensure!(
                    status == StatusCode::CONFLICT,
                    "public upload cancellation did not settle"
                );
                let current: ArtifactUploadSession = client
                    .get(&url)
                    .bearer_auth(&token)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                Ok(Settlement::Observed(Box::new(current)))
            }));
        }
        // Admission can precede cleanup by the entire upload duration. Latch
        // the original owner's cap only when cleanup begins; interrupted waits
        // retain this cap and the same consuming request future.
        let owner_cap = owner::cleanup_deadline()?.into();
        let deadline = state
            .deadline
            .map_or(owner_cap, |first| first.min(owner_cap));
        state.deadline = Some(deadline);
        // timeout_at may poll a ready future before checking its timer. An
        // already expired retained cap must fail before that future is polled.
        if tokio::time::Instant::now() >= deadline {
            state.failed = true;
            self.journal.append(&Record::Cleanup {
                upload_id: Some(id),
                status: CleanupStatus::Failed,
            })?;
            anyhow::bail!("public upload cleanup original deadline expired");
        }
        let result =
            tokio::time::timeout_at(deadline, state.future.as_mut().expect("retained cancel"))
                .await;
        let outcome = match result {
            Ok(Ok(Settlement::Cancelled)) => Ok(CleanupStatus::Cancelled),
            Ok(Ok(Settlement::Observed(current))) => (|| {
                if let Some(receipt) = &current.receipt {
                    self.record_observation(receipt)?;
                }
                ensure!(
                    current.upload_id == id
                        && current.state
                            == veoveo_artifact_contract::ArtifactUploadState::Completed
                        && current
                            .receipt
                            .as_ref()
                            .is_some_and(|receipt| receipt.upload_id == id),
                    "public upload cancellation conflict has no completed receipt"
                );
                self.record_publication(current.receipt.as_ref().expect("admitted receipt"))?;
                Ok(CleanupStatus::RetainedPublished)
            })(),
            _ => Err(anyhow::anyhow!(
                "public upload cleanup failed or exceeded original deadline"
            )),
        };
        state.failed = outcome.is_err();
        state.done = !state.failed;
        let recorded = self.journal.append(&Record::Cleanup {
            upload_id: Some(id),
            status: outcome.unwrap_or(CleanupStatus::Failed),
        });
        if recorded.is_err() {
            state.failed = true;
            state.done = false;
        }
        recorded?;
        ensure!(
            !state.failed,
            "public upload cleanup failed or exceeded original deadline"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    /// Each mode owns a separate global lifecycle owner. The loopback server
    /// observes the actual DELETE, rather than mirroring cleanup state.
    #[tokio::test]
    async fn public_upload_registered_cleanup_is_owned_and_sticky() -> Result<()> {
        const MODE: &str = "VEOVEO_ARTIFACT_PUBLIC_CLEANUP_CONTROL";
        const ROOT: &str = "VEOVEO_ARTIFACT_PUBLIC_CLEANUP_ROOT";
        if let Ok(mode) = std::env::var(MODE) {
            let root = PathBuf::from(std::env::var_os(ROOT).context("control root")?);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let base = format!("http://{}/uploads", listener.local_addr()?);
            let id = ArtifactUploadId::new();
            let expected_path = format!("DELETE /uploads/{id} HTTP/1.1");
            let failed = mode == "failed";
            let unknown = mode == "unknown";
            let delayed = mode == "delayed";
            let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                if unknown {
                    ensure!(
                        tokio::time::timeout(Duration::from_millis(300), listener.accept())
                            .await
                            .is_err(),
                        "unacknowledged upload caused a guessed cleanup request"
                    );
                    return Ok::<_, anyhow::Error>(());
                }
                let (mut socket, _) =
                    tokio::time::timeout(Duration::from_secs(4), listener.accept()).await??;
                let mut request = Vec::new();
                loop {
                    let mut byte = [0u8];
                    ensure!(
                        socket.read(&mut byte).await? == 1,
                        "incomplete control request"
                    );
                    request.push(byte[0]);
                    ensure!(
                        request.len() <= 8192,
                        "control request exceeds header bound"
                    );
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                ensure!(
                    request.starts_with(expected_path.as_bytes()),
                    "wrong cleanup method or identity"
                );
                if delayed {
                    let _ = seen_tx.send(());
                    release_rx.await?;
                }
                let status = if failed {
                    "500 Internal Server Error"
                } else {
                    "204 No Content"
                };
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .await?;
                socket.shutdown().await?;
                ensure!(
                    tokio::time::timeout(Duration::from_millis(300), listener.accept())
                        .await
                        .is_err(),
                    "cleanup replayed a consumed mutation"
                );
                Ok(())
            });
            let journal = Journal::create(&root.join("evidence.json"), true)?;
            let observed = journal.clone();
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .redirect(Policy::none())
                .build()?;
            let result: Result<()> = owner::run(async {
                let descriptor = CreateArtifactUpload {
                    filename: "fixture.bin".into(),
                    mime_type: "application/octet-stream".into(),
                    byte_len: Some(2048),
                    sha256: None,
                };
                let upload = observed.begin(
                    &client,
                    "private-synthetic-marker",
                    &base,
                    "fixture-key",
                    &descriptor,
                )?;
                if !unknown {
                    upload.acknowledge(id)?;
                }
                ensure!(
                    upload.cleanup.state.lock().await.deadline.is_none(),
                    "session acquisition prematurely started cleanup interval"
                );
                if delayed {
                    tokio::select! {
                        result = upload.cancel() => {
                            result?;
                            anyhow::bail!("controlled cleanup completed before response release");
                        }
                        result = seen_rx => { result?; }
                    }
                    let first = upload
                        .cleanup
                        .state
                        .lock()
                        .await
                        .deadline
                        .context("actual cleanup did not latch its deadline")?;
                    release_tx
                        .send(())
                        .map_err(|_| anyhow::anyhow!("control response released early"))?;
                    upload.cancel().await?;
                    ensure!(
                        upload.cleanup.state.lock().await.deadline == Some(first),
                        "resumed cleanup acquired a fresh interval"
                    );
                    return Ok(());
                }
                if failed || unknown {
                    ensure!(
                        upload.cancel().await.is_err(),
                        "failed/unresolved cleanup settled"
                    );
                    ensure!(upload.cancel().await.is_err(), "sticky failure disappeared");
                    return Ok(());
                }
                let mut command = tokio::process::Command::new("sh");
                command
                    .args([
                        "-c",
                        "sleep 0.1; kill -INT \"$1\"; exec sleep 30",
                        "fixture",
                    ])
                    .arg(std::process::id().to_string())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                let _child = veoveo_testing_support::spawn_async(command)?;
                std::future::pending::<Result<()>>().await
            })
            .await;
            ensure!(
                result.is_ok() == delayed,
                "owner misclassified completed/cancelled/unresolved cleanup"
            );
            tokio::time::timeout(Duration::from_secs(5), server).await???;
            let records = fs::read_to_string(root.join("evidence.uploads.jsonl"))?;
            ensure!(
                !records.contains("private-synthetic-marker"),
                "journal exposed bearer credential"
            );
            let last: Value =
                serde_json::from_str(records.lines().last().context("cleanup record missing")?)?;
            ensure!(
                last["phase"] == "cleanup",
                "missing actual cleanup observation"
            );
            let expected = match mode.as_str() {
                "global" | "delayed" => "cancelled",
                "failed" => "failed",
                "unknown" => "unresolvedCreation",
                _ => anyhow::bail!("unknown cleanup control"),
            };
            ensure!(
                last["status"] == expected,
                "cleanup outcome was misclassified"
            );
            return Ok(());
        }
        for mode in ["global", "failed", "unknown", "delayed"] {
            let directory = tempfile::tempdir()?;
            fs::create_dir(directory.path().join("groups"))?;
            let mut command = tokio::process::Command::new(std::env::current_exe()?);
            command
                .args([
                    "public_upload_registered_cleanup_is_owned_and_sticky",
                    "--nocapture",
                ])
                .env(MODE, mode)
                .env(ROOT, directory.path())
                .env("VEOVEO_SMOKE_LOCAL_GROUPS", directory.path().join("groups"))
                .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "2");
            let output =
                veoveo_testing_support::output_async(command, Duration::from_secs(15)).await?;
            ensure!(
                output.status.success(),
                "public upload cleanup control {mode} failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }
    #[tokio::test]
    async fn public_upload_expiry_and_publication_survive_interruption() -> Result<()> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        const MODE: &str = "VEOVEO_ARTIFACT_PUBLIC_RECEIPT_CONTROL";
        const ROOT: &str = "VEOVEO_ARTIFACT_PUBLIC_RECEIPT_ROOT";
        if let Ok(mode) = std::env::var(MODE) {
            let root = PathBuf::from(std::env::var_os(ROOT).context("control root")?);
            let path = root.join("receipt.uploads.jsonl");
            let journal = Journal::create(&root.join("receipt.json"), true)?;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let base = format!("http://{}/uploads", listener.local_addr()?);
            let id = ArtifactUploadId::new();
            let artifact_id = veoveo_artifact_contract::ArtifactId::new();
            let receipt = ArtifactUploadReceipt {
                upload_id: if mode == "mismatch" {
                    ArtifactUploadId::new()
                } else {
                    id
                },
                artifact_id,
                artifact_uri: veoveo_artifact_contract::ArtifactUri::plane(artifact_id),
                sha256: UploadSha256::parse(hex::encode(Sha256::digest(FOCUSED_BYTES)))?,
                byte_len: 2048,
                mime_type: "application/octet-stream".into(),
                filename: "fixture.bin".into(),
                created_at: chrono::Utc::now(),
            };
            let descriptor = CreateArtifactUpload {
                filename: receipt.filename.clone(),
                mime_type: receipt.mime_type.clone(),
                byte_len: Some(receipt.byte_len),
                sha256: Some(receipt.sha256.clone()),
            };
            let current = ArtifactUploadSession {
                upload_id: id,
                state: veoveo_artifact_contract::ArtifactUploadState::Completed,
                descriptor: descriptor.clone(),
                layout: veoveo_artifact_contract::UploadLayout {
                    part_bytes: std::num::NonZeroU64::new(2048).unwrap(),
                    max_parts: NonZeroU32::new(1).unwrap(),
                    max_total_bytes: std::num::NonZeroU64::new(2048).unwrap(),
                    parallel_parts: NonZeroU32::new(1).unwrap(),
                },
                accepted_bytes: 2048,
                accepted_part_count: 1,
                parts: vec![],
                next_part_cursor: None,
                created_at: receipt.created_at,
                expires_at: receipt.created_at + chrono::Duration::minutes(5),
                receipt: Some(receipt.clone()),
                failure: None,
            };
            let server = if mode == "reconciled" || mode == "mismatch" {
                let mismatch = mode == "mismatch";
                Some(tokio::spawn(async move {
                    let requests = if mismatch {
                        vec!["POST", "PUT", "PUT", "PUT", "POST", "DELETE"]
                    } else {
                        vec!["DELETE", "GET"]
                    };
                    for (index, method) in requests.into_iter().enumerate() {
                        let (mut socket, _) = listener.accept().await?;
                        let request = control_request(&mut socket).await?;
                        let expected = if mismatch {
                            match index {
                                0 => "POST /uploads HTTP/1.1".into(),
                                1..=3 => format!("PUT /uploads/{id}/parts/1 HTTP/1.1"),
                                4 => format!("POST /uploads/{id}/complete HTTP/1.1"),
                                _ => format!("DELETE /uploads/{id} HTTP/1.1"),
                            }
                        } else {
                            format!("{method} /uploads/{id} HTTP/1.1")
                        };
                        ensure!(
                            request.starts_with(expected.as_bytes()),
                            "receipt reconciliation changed request identity/order"
                        );
                        let (status, body) = if mismatch {
                            match index {
                                0 => {
                                    let mut open = current.clone();
                                    open.state =
                                        veoveo_artifact_contract::ArtifactUploadState::Open;
                                    open.receipt = None;
                                    open.accepted_bytes = 0;
                                    open.accepted_part_count = 0;
                                    ("201 Created", serde_json::to_vec(&open)?)
                                }
                                1 | 2 => (
                                    "200 OK",
                                    serde_json::to_vec(&UploadPartReceipt {
                                        part_number: NonZeroU32::new(1).unwrap(),
                                        byte_len: 2048,
                                        sha256: current
                                            .receipt
                                            .as_ref()
                                            .expect("fixture receipt")
                                            .sha256
                                            .clone(),
                                    })?,
                                ),
                                3 => ("409 Conflict", Vec::new()),
                                4 => ("200 OK", serde_json::to_vec(&current)?),
                                _ => ("500 Internal Server Error", Vec::new()),
                            }
                        } else if method == "DELETE" {
                            ("409 Conflict", Vec::new())
                        } else {
                            ("200 OK", serde_json::to_vec(&current)?)
                        };
                        socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await?;
                        socket.write_all(&body).await?;
                        socket.shutdown().await?;
                    }
                    Ok::<_, anyhow::Error>(())
                }))
            } else {
                None
            };
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(1))
                .redirect(Policy::none())
                .build()?;
            let result: Result<()> = owner::run(async {
                if mode == "mismatch" {
                    ensure!(
                        super::super::upload_small(
                            &client,
                            "synthetic-private",
                            &base,
                            super::super::SmallUploadFixture {
                                filename: "fixture.bin",
                                mime: "application/octet-stream",
                                bytes: FOCUSED_BYTES.to_vec(),
                                known: true,
                            },
                            &journal
                        )
                        .await
                        .is_err(),
                        "mismatched completion was qualified"
                    );
                    let records = fs::read_to_string(&path)?
                        .lines()
                        .map(serde_json::from_str::<Value>)
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    let observed = records
                        .iter()
                        .find(|record| record["phase"] == "received")
                        .context("production assertion discarded received receipt")?;
                    ensure!(
                        serde_json::from_value::<ArtifactUploadReceipt>(
                            observed["receipt"].clone()
                        )? == receipt,
                        "production assertion lost mismatched receipt identity"
                    );
                    ensure!(
                        !records.iter().any(|record| record["phase"] == "published"),
                        "mismatched receipt settled publication"
                    );
                    ensure!(
                        records
                            .last()
                            .is_some_and(|record| record["status"] == "failed"),
                        "failed cleanup was not preserved after mismatched observation"
                    );
                    return Ok(());
                }
                let upload = journal.begin(
                    &client,
                    "synthetic-private",
                    &base,
                    "receipt-key",
                    &descriptor,
                )?;
                upload.acknowledge(id)?;
                if mode == "expired" {
                    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
                    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
                    let polled_ready = Arc::new(AtomicUsize::new(0));
                    let counted = polled_ready.clone();
                    upload.cleanup.state.lock().await.future = Some(Box::pin(async move {
                        let _ = started_tx.send(());
                        ready_rx.await?;
                        counted.fetch_add(1, Ordering::SeqCst);
                        Ok(Settlement::Cancelled)
                    }));
                    tokio::select! {
                        result = upload.cancel() => {
                            result?;
                            anyhow::bail!("pending original cleanup unexpectedly settled");
                        }
                        result = started_rx => { result?; }
                    }
                    {
                        let mut state = upload.cleanup.state.lock().await;
                        ensure!(
                            state.deadline.is_some(),
                            "interrupted cleanup lost original cap"
                        );
                        // Advance only the retained cap to model elapsed time,
                        // without a long wall-clock wait or a second job.
                        state.deadline = Some(tokio::time::Instant::now() - Duration::from_secs(1));
                    }
                    ready_tx
                        .send(())
                        .map_err(|_| anyhow::anyhow!("retained waiter lost"))?;
                    ensure!(
                        upload.cancel().await.is_err(),
                        "expired ready future settled"
                    );
                    ensure!(
                        upload.cancel().await.is_err(),
                        "expiry failure was not sticky"
                    );
                    ensure!(
                        polled_ready.load(Ordering::SeqCst) == 0,
                        "expired future was polled ready"
                    );
                    return Ok(());
                }
                if mode == "journal" {
                    *journal.0.lock().expect("journal") = fs::File::open(&path)?;
                    ensure!(
                        upload.received(&receipt).is_err(),
                        "read-only journal accepted observation"
                    );
                    ensure!(
                        upload.cleanup.state.lock().await.failed,
                        "journal failure was not sticky"
                    );
                    ensure!(
                        upload
                            .cleanup
                            .observations
                            .lock()
                            .expect("publication")
                            .contains(&receipt),
                        "journal failure lost observed publication"
                    );
                    ensure!(
                        upload.cancel().await.is_err(),
                        "failed publication was retried as cleanup"
                    );
                    return Ok(());
                }
                if mode == "reconciled" {
                    upload.cancel().await?;
                } else {
                    upload.received(&receipt)?;
                    upload.published(&receipt)?;
                }
                ensure!(
                    upload
                        .cleanup
                        .publication
                        .lock()
                        .expect("publication")
                        .as_ref()
                        == Some(&receipt),
                    "known publication was not retained"
                );
                let records = fs::read_to_string(&path)?;
                let publications = records
                    .lines()
                    .map(serde_json::from_str::<Value>)
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let recorded = publications
                    .iter()
                    .find(|record| record["phase"] == "published")
                    .context("full publication not durable before next effect")?;
                ensure!(
                    serde_json::from_value::<ArtifactUploadReceipt>(recorded["receipt"].clone())?
                        == receipt,
                    "journal lost the full known publication"
                );
                if mode == "published" {
                    anyhow::bail!("controlled later assertion failure");
                }
                Ok(())
            })
            .await;
            ensure!(
                result.is_ok() == (mode == "reconciled"),
                "publication/expiry result misclassified"
            );
            if let Some(server) = server {
                tokio::time::timeout(Duration::from_secs(3), server).await???;
            }
            return Ok(());
        }
        for mode in ["expired", "published", "journal", "reconciled", "mismatch"] {
            let directory = tempfile::tempdir()?;
            fs::create_dir(directory.path().join("groups"))?;
            let mut command = tokio::process::Command::new(std::env::current_exe()?);
            command
                .args([
                    "public_upload_expiry_and_publication_survive_interruption",
                    "--nocapture",
                ])
                .env(MODE, mode)
                .env(ROOT, directory.path())
                .env("VEOVEO_SMOKE_LOCAL_GROUPS", directory.path().join("groups"))
                .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "2");
            let output =
                veoveo_testing_support::output_async(command, Duration::from_secs(15)).await?;
            ensure!(
                output.status.success(),
                "publication/expiry control {mode} failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }
    async fn control_request(socket: &mut tokio::net::TcpStream) -> Result<Vec<u8>> {
        let mut headers = Vec::new();
        loop {
            let mut byte = [0];
            ensure!(
                socket.read(&mut byte).await? == 1,
                "incomplete control request"
            );
            headers.push(byte[0]);
            ensure!(headers.len() <= 8192, "control headers exceed bound");
            if headers.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let text = std::str::from_utf8(&headers)?;
        let length = text
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map(|(_, value)| value.trim().parse::<usize>())
            .transpose()?
            .unwrap_or(0);
        ensure!(length <= 16 * 1024, "control body exceeds bound");
        let mut body = vec![0; length];
        socket.read_exact(&mut body).await?;
        Ok(headers)
    }
}
