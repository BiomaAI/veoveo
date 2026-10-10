//! Typed public Gateway admin requests; no response/error body enters diagnostics.
use super::*;
use serde::de::DeserializeOwned;
#[derive(Clone, Serialize)]
#[serde(tag = "operation", content = "request", rename_all = "snake_case")]
pub(super) enum Request {
    Sources,
    SourceCreate(CreateSourceRequest),
    AcquisitionCreate(CreateAcquisitionRequest),
    AcquisitionGet(TimeAcquisitionId),
    ReleaseGet(AuthorityReleaseId),
    Active,
    Activate {
        id: AuthorityReleaseId,
        request: ActivateReleaseRequest,
    },
    EpochCreate(UpsertMissionEpochRequest),
}
impl Request {
    fn path(&self) -> Vec<&str> {
        match self {
            Self::Sources | Self::SourceCreate(_) => vec!["sources"],
            Self::AcquisitionCreate(_) => vec!["acquisitions"],
            Self::AcquisitionGet(id) => vec!["acquisitions", id.as_str()],
            Self::ReleaseGet(id) => vec!["releases", id.as_str()],
            Self::Active => vec!["active-authorities"],
            Self::Activate { id, .. } => vec!["releases", id.as_str(), "activate"],
            Self::EpochCreate(_) => vec!["epochs"],
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(super) enum Success {
    Sources(AdminPage<TimeSource>),
    Source(TimeSource),
    Acquisition(TimeAcquisition),
    Release(AuthorityRelease),
    Active(AdminPage<ActiveAuthoritySelection>),
    Epoch(MissionEpoch),
}
pub(super) trait Receipt: DeserializeOwned {
    fn receipt(&self) -> Success;
}
impl Receipt for AdminPage<TimeSource> {
    fn receipt(&self) -> Success {
        Success::Sources(self.clone())
    }
}
impl Receipt for TimeSource {
    fn receipt(&self) -> Success {
        Success::Source(self.clone())
    }
}
impl Receipt for TimeAcquisition {
    fn receipt(&self) -> Success {
        let mut safe = self.clone();
        safe.message = "acquisition status text omitted".to_owned();
        Success::Acquisition(safe)
    }
}
impl Receipt for AuthorityRelease {
    fn receipt(&self) -> Success {
        Success::Release(self.clone())
    }
}
impl Receipt for AdminPage<ActiveAuthoritySelection> {
    fn receipt(&self) -> Success {
        Success::Active(self.clone())
    }
}
impl Receipt for MissionEpoch {
    fn receipt(&self) -> Success {
        Success::Epoch(self.clone())
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Observation {
    request: Request,
    status: Option<u16>,
    code: Option<AdminErrorCode>,
    complete: bool,
    acknowledged: Option<Success>,
    response_sha256: Option<veoveo_types::Sha256Digest>,
    failure_sha256: Option<veoveo_types::Sha256Digest>,
}
#[derive(Clone)]
struct Captured {
    status: u16,
    complete: bool,
    code: Option<AdminErrorCode>,
    acknowledged: Option<Success>,
    response_sha256: Option<veoveo_types::Sha256Digest>,
}
#[derive(Default)]
struct Retained {
    completed: std::sync::Mutex<std::collections::BTreeMap<usize, Captured>>,
    flush_failed: std::sync::atomic::AtomicBool,
}
#[derive(Clone)]
pub(super) struct Admin {
    origin: reqwest::Url,
    profile: veoveo_types::GatewayProfileId,
    bearer: reqwest::header::HeaderValue,
    client: reqwest::Client,
    journal: Arc<Mutex<Journal>>,
    pub deadline: tokio::time::Instant,
    retained: Arc<Retained>,
}
impl Admin {
    pub fn new(
        input: &Input,
        journal: Arc<Mutex<Journal>>,
        deadline: tokio::time::Instant,
    ) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Ok(Self {
            origin: reqwest::Url::parse(input.installation.endpoint.as_str())?,
            profile: input.administrator.profile.clone(),
            bearer: installed::bearer_header(&input.administrator.token_file)?,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            journal,
            deadline,
            retained: Arc::new(Retained::default()),
        })
    }
    pub async fn request<T: Receipt>(
        &self,
        request: Request,
    ) -> Result<(u16, Option<T>, Option<AdminErrorCode>)> {
        ensure!(
            tokio::time::Instant::now() < self.deadline,
            "authority original operation deadline expired"
        );
        let index = {
            let mut journal = self.journal.lock().await;
            let index = journal.evidence.http.len();
            ensure!(index < 512, "authority HTTP request budget exhausted");
            journal.evidence.http.push(Observation {
                request: request.clone(),
                status: None,
                code: None,
                complete: false,
                acknowledged: None,
                response_sha256: None,
                failure_sha256: None,
            });
            journal.persist()?;
            index
        };
        let result = tokio::time::timeout_at(self.deadline, self.send::<T>(&request, index))
            .await
            .map_err(|_| anyhow::anyhow!("authority original request deadline expired"))
            .and_then(|result| result);
        if let Err(error) = &result {
            let mut journal = self.journal.lock().await;
            journal.evidence.http[index].failure_sha256 = Some(hash(error.to_string().as_bytes()));
            journal.persist()?;
        }
        result.map_err(|_| {
            anyhow::anyhow!("authority admin request failed; inspect private safe observation")
        })
    }
    async fn send<T: Receipt>(
        &self,
        request: &Request,
        index: usize,
    ) -> Result<(u16, Option<T>, Option<AdminErrorCode>)> {
        let mut url = self.origin.clone();
        url.set_path("/");
        url.set_query(None);
        url.set_fragment(None);
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid public admin origin"))?
            .clear()
            .extend(["admin", self.profile.as_str(), "servers", "time"])
            .extend(request.path());
        let builder = match request {
            Request::SourceCreate(value) => self.client.post(url).json(value),
            Request::AcquisitionCreate(value) => self.client.post(url).json(value),
            Request::Activate { request, .. } => self.client.post(url).json(request),
            Request::EpochCreate(value) => self.client.post(url).json(value),
            _ => self.client.get(url),
        };
        owner::check_effect()?;
        ensure!(
            tokio::time::Instant::now() < self.deadline,
            "authority original operation deadline expired"
        );
        let mut response = builder
            .header(reqwest::header::AUTHORIZATION, self.bearer.clone())
            .send()
            .await?;
        let status = response.status().as_u16();
        self.retained
            .completed
            .lock()
            .expect("retained response lock")
            .insert(
                index,
                Captured {
                    status,
                    complete: false,
                    code: None,
                    acknowledged: None,
                    response_sha256: None,
                },
            );
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                body.len() + chunk.len() <= 1024 * 1024,
                "authority admin response oversized"
            );
            body.extend_from_slice(&chunk);
        }
        // Complete bytes and every decoded identity belong to Admin's outer owner
        // before any journal await can be interrupted by a sibling or cancellation.
        let decoded: Result<(Option<T>, Option<AdminErrorCode>)> = if (200..300).contains(&status) {
            serde_json::from_slice(&body)
                .map(|value| (Some(value), None))
                .map_err(Into::into)
        } else {
            serde_json::from_slice::<AdminError>(&body)
                .map(|error| (None, Some(error.code)))
                .map_err(Into::into)
        };
        self.retained
            .completed
            .lock()
            .expect("retained response lock")
            .insert(
                index,
                Captured {
                    status,
                    complete: true,
                    code: decoded.as_ref().ok().and_then(|(_, code)| *code),
                    acknowledged: decoded
                        .as_ref()
                        .ok()
                        .and_then(|(value, _)| value.as_ref().map(Receipt::receipt)),
                    response_sha256: Some(hash(&body)),
                },
            );
        self.flush().await?;
        let (value, code) = decoded?;
        Ok((status, value, code))
    }
    pub async fn flush(&self) -> Result<()> {
        use std::sync::atomic::Ordering;
        let mut journal = self.journal.lock().await;
        {
            let captured = self
                .retained
                .completed
                .lock()
                .expect("retained response lock");
            for (index, response) in captured.iter() {
                let observation = &mut journal.evidence.http[*index];
                observation.status = Some(response.status);
                observation.complete = response.complete;
                observation.code = response.code;
                observation.acknowledged = response.acknowledged.clone();
                observation.response_sha256 = response.response_sha256.clone();
            }
        }
        if let Err(error) = journal.persist() {
            self.retained.flush_failed.store(true, Ordering::Release);
            return Err(error);
        }
        ensure!(
            !self.retained.flush_failed.load(Ordering::Acquire),
            "authority journal flush previously failed"
        );
        Ok(())
    }
    pub async fn ok<T: Receipt>(&self, request: Request) -> Result<T> {
        let (status, value, _) = self.request(request).await?;
        ensure!(
            (200..300).contains(&status),
            "authority admin success required"
        );
        value.context("authority admin successful body absent")
    }
}
pub(super) async fn active(admin: &Admin) -> Result<Vec<ActiveAuthoritySelection>> {
    let page: AdminPage<ActiveAuthoritySelection> = admin.ok(Request::Active).await?;
    ensure!(
        page.next_cursor.is_none() && page.items.len() <= 2,
        "active selections must be complete"
    );
    let mut kinds = Vec::new();
    for entry in &page.items {
        ensure!(
            !kinds.contains(&entry.release.dataset_kind),
            "duplicate active family"
        );
        kinds.push(entry.release.dataset_kind);
    }
    Ok(page.items)
}
pub(super) fn binding(
    selected: &[ActiveAuthoritySelection],
    family: AuthorityDatasetKind,
) -> Result<&ActiveAuthoritySelection> {
    selected
        .iter()
        .find(|entry| entry.release.dataset_kind == family)
        .context("active family absent")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, extract::State, routing::post};
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn complete_ack_survives_blocked_journal_and_cancelled_sibling() -> Result<()> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (input, _) = super::super::tests::fixture()?;
        let directory = tempfile::tempdir()?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        let path = directory.path().join("authority.jsonl");
        let journal = Arc::new(Mutex::new(Journal {
            file: open_receipt(&path)?,
            evidence: Evidence {
                schema: "veoveo.ai/time-authority-acceptance/v1",
                phase: Phase::Admitted,
                isolation: input.isolation.clone(),
                initial_authority: input.initial_authority.clone(),
                initial_epoch: input.epoch.clone(),
                http: vec![],
                active_snapshots: vec![],
                delivered_changes: vec![],
                subscription_id: None,
                subscription_filter: None,
                initial_subscription_authority: None,
                trace: trace::Trace::default(),
                listener_closed: true,
                caller_closed: true,
                failed: false,
                failure_sha256: None,
                remaining_gates: [
                    "isolated_retirement",
                    "acquisition_recovery",
                    "authority_restart",
                ],
            },
        }));
        let acquisition: TimeAcquisition = serde_json::from_value(serde_json::json!({
            "acquisitionId":format!("time-acquisition-{}",uuid::Uuid::now_v7()),
            "sourceId":input.tzdb.create.source.source_id,
            "expectedSourceDigestSha256":input.tzdb.digest,
            "status":"queued","phase":"queued","stagedReleaseId":null,
            "message":"SECRET_PROVIDER_ERROR_MARKER", "createdAt":"2026-10-10T00:00:00Z",
            "updatedAt":"2026-10-10T00:00:00Z","recordVersion":1
        }))?;
        let body = serde_json::to_vec(&acquisition)?;
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (body_tx, body_rx) = tokio::sync::oneshot::channel();
        type Reply = (
            tokio::sync::oneshot::Sender<()>,
            tokio::sync::oneshot::Receiver<Vec<u8>>,
        );
        let state = Arc::new(Mutex::new(Some((started_tx, body_rx))));
        let router = Router::new()
            .route(
                "/admin/operator/servers/time/acquisitions",
                post(
                    |State(state): State<Arc<Mutex<Option<Reply>>>>| async move {
                        let (started, body) = state.lock().await.take().unwrap();
                        let _ = started.send(());
                        body.await.unwrap()
                    },
                ),
            )
            .with_state(state);
        let socket = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = socket.local_addr()?;
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let admin = Admin {
            origin: reqwest::Url::parse(&format!("http://{address}"))?,
            profile: "operator".parse()?,
            bearer: reqwest::header::HeaderValue::from_static("Bearer inert-fixture"),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()?,
            journal: journal.clone(),
            deadline: tokio::time::Instant::now() + Duration::from_secs(5),
            retained: Arc::new(Retained::default()),
        };
        let mut server = tokio::spawn(async move {
            axum::serve(socket, router)
                .with_graceful_shutdown(async {
                    let _ = stop_rx.await;
                })
                .await
        });
        let result = tokio::time::timeout_at(admin.deadline, async {
            let request: CreateAcquisitionRequest = CreateAcquisitionRequestValue {
                source_id: acquisition.source_id.clone(),
                expected_source_digest_sha256: acquisition.expected_source_digest_sha256.clone(),
                idempotency_key: "ack-retention-control".to_owned(),
            }.try_into()?;
            let mut request = Box::pin(admin.request::<TimeAcquisition>(Request::AcquisitionCreate(request)));
            tokio::select! {
                _ = started_rx => (),
                _ = &mut request => anyhow::bail!("request completed before selected response seam"),
            }
            let blocked = journal.lock().await;
            ensure!(blocked.evidence.http.len()==1 && blocked.evidence.http[0].acknowledged.is_none());
            body_tx.send(body).map_err(|_| anyhow::anyhow!("fixture response receiver lost"))?;
            let sibling = async {
                loop {
                    let complete = admin.retained.completed.lock().unwrap().get(&0)
                        .is_some_and(|captured| captured.complete);
                    if complete { anyhow::bail!("selected concurrent sibling failure"); }
                    tokio::task::yield_now().await;
                }
            };
            let outcome: Result<((), ())> = tokio::try_join!(async { (&mut request).await?; Ok(()) }, sibling);
            ensure!(outcome.is_err());
            // The operation has been dropped while its journal write was blocked.
            drop(request);
            ensure!(blocked.evidence.http[0].acknowledged.is_none());
            drop(blocked);
            admin.flush().await?;
            let saved = journal.lock().await;
            let Some(Success::Acquisition(known)) = &saved.evidence.http[0].acknowledged else {
                anyhow::bail!("known acquisition identity was lost on request cancellation");
            };
            ensure!(known.acquisition_id==acquisition.acquisition_id
                && known.source_id==acquisition.source_id && known.status==acquisition.status
                && known.phase==acquisition.phase && known.message=="acquisition status text omitted");
            ensure!(acquisition.message=="SECRET_PROVIDER_ERROR_MARKER");
            ensure!(!fs::read_to_string(&path)?.contains("SECRET_PROVIDER_ERROR_MARKER"));
            Ok(())
        }).await;
        let _ = stop_tx.send(());
        let closed = tokio::time::timeout(Duration::from_secs(2), &mut server).await;
        if closed.is_err() {
            server.abort();
            let _ = server.await;
        }
        ensure!(
            matches!(closed, Ok(Ok(Ok(())))),
            "owned fixture server cleanup failed"
        );
        result.context("ack-retention control deadline")?
    }
}
