//! One externally applied process crash, with retained native observation ownership.
use super::*;
use std::{
    future::Future,
    io::{Read, Seek, SeekFrom},
    pin::Pin,
    sync::Arc,
};
use tokio::sync::Mutex;
use veoveo_gateway_contract::ProtectedResourceId;
use veoveo_testing_support::{
    installed::restart::{CrashIdentity, CrashReceipt, CrashTarget, CrashWatch, DeploymentRestart},
    lifecycle::owner::{self, CleanupKind, CleanupRegistration},
};

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Schema {
    #[vocabulary(rename = "veoveo.ai/timeseries-process-crash/v1")]
    V1,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Fixture {
    schema: Schema,
    context: String,
    namespace: String,
    operator_resource: ProtectedResourceId,
    pub target: CrashTarget,
}
impl Fixture {
    pub fn validate(&self) -> Result<()> {
        self.target.validate()?;
        ensure!(
            self.schema == Schema::V1 && !self.context.is_empty() && !self.namespace.is_empty(),
            "Timeseries crash requires an installation context/namespace"
        );
        ensure!(
            self.target.container == "timeseries-mcp",
            "Timeseries crash must select the Rust server container"
        );
        Ok(())
    }
    pub fn admit(&self, installation: &support::InstalledTarget) -> Result<()> {
        self.admit_installation(&installation.target, &installation.operator.resource)
    }
    fn admit_installation(
        &self,
        target: &veoveo_deploy_contract::InstallationTarget,
        resource: &ProtectedResourceId,
    ) -> Result<()> {
        self.validate()?;
        target.validate()?;
        ensure!(
            self.context == target.kubernetes.context
                && self.namespace == target.kubernetes.namespace
                && self.operator_resource == *resource,
            "Timeseries crash fixture belongs to another installation/caller route"
        );
        ensure!(
            target
                .expected_deployments
                .contains(&self.target.deployment),
            "Timeseries crash Deployment is not declared by the installation"
        );
        Ok(())
    }
}
pub(super) fn driver(
    installation: &support::InstalledTarget,
    fixture: &Fixture,
    client: &SmokeMcpClient,
) -> Result<DeploymentRestart> {
    DeploymentRestart::new(
        &installation.target,
        &fixture.target.deployment,
        "timeseries-mcp",
        client.peer().clone(),
        veoveo_timeseries_mcp::contract::TimeseriesResource::Contract.to_uri()?,
    )
}
/// A failed fresh original-instance fence must not publish permission to crash.
pub(super) async fn ready_for_crash(
    admission: impl Future<Output = Result<()>>,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<()> {
    admission.await?;
    observe(file, receipt, Step::ReadyForCrash, None)
}
#[derive(Clone, Copy, Default, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Close {
    #[default]
    NotOpened,
    Open,
    Pending,
    Closed,
    Failed,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Journal {
    identity: Option<CrashIdentity>,
    receipt: Option<CrashReceipt>,
    close: Close,
    #[serde(default)]
    journal_write_failed: bool,
}
type Closing = Pin<Box<dyn Future<Output = bool> + Send>>;
pub(super) struct Handles {
    pub watch: Option<CrashWatch>,
    closing: Option<Closing>,
    journal: Journal,
    end: Option<tokio::time::Instant>,
}
impl Handles {
    pub fn sync(&mut self, receipt: &mut Receipt) -> Result<()> {
        if let Some(watch) = &self.watch {
            self.journal.identity = Some(watch.identity()?);
            self.journal.receipt = watch.snapshot();
            self.journal.close = Close::Open;
        }
        if let Some(journal) = receipt
            .lifecycle
            .as_mut()
            .and_then(|lifecycle| lifecycle.crash.as_mut())
        {
            *journal = self.journal.clone();
        }
        Ok(())
    }
    pub fn closed(&self) -> bool {
        matches!(self.journal.close, Close::NotOpened | Close::Closed)
            && self.watch.is_none()
            && self.closing.is_none()
    }
    pub async fn close(&mut self, file: &mut std::fs::File) -> Result<()> {
        // Latch the owning cleanup cap before journal I/O or consuming the watch.
        if self.end.is_none() && (self.watch.is_some() || self.closing.is_some()) {
            self.end = Some(tokio::time::Instant::from_std(owner::cleanup_deadline()?));
        }
        // Receipt failures never prevent teardown of the retained native handle.
        if let Some(watch) = &self.watch {
            self.journal.identity = watch.identity().ok();
            self.journal.receipt = watch.snapshot();
            self.journal.close = Close::Pending;
        }
        if self.watch.is_some() || self.closing.is_some() {
            self.journal.journal_write_failed |= self.write(file).is_err();
        }
        if self.closing.is_none()
            && let Some(watch) = self.watch.take()
        {
            self.closing = Some(Box::pin(watch.close()));
        }
        let close_result = async {
            ensure!(
                self.journal.close != Close::Failed,
                "Timeseries native watch close remains unproven"
            );
            if let Some(close) = self.closing.as_mut() {
                let end = self
                    .end
                    .context("Timeseries native watch cleanup cap absent")?;
                if tokio::time::Instant::now() >= end {
                    self.journal.close = Close::Failed;
                    bail!("Timeseries native watch close deadline; outcome unproven");
                }
                match tokio::time::timeout_at(end, close).await {
                    Ok(true) => {
                        self.closing = None;
                        self.journal.close = Close::Closed;
                    }
                    _ => {
                        self.journal.close = Close::Failed;
                        bail!("Timeseries native watch close failed/deadline");
                    }
                }
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if self.journal.close != Close::NotOpened {
            self.journal.journal_write_failed |= self.write(file).is_err();
        }
        match (close_result, self.journal.journal_write_failed) {
            (Err(_), true) => bail!(
                "Timeseries native watch cleanup and receipt persistence failed; outcome unproven"
            ),
            (Err(error), false) => Err(error),
            (Ok(()), true) => {
                bail!("Timeseries native watch closed but receipt persistence failed")
            }
            (Ok(()), false) => Ok(()),
        }
    }
    fn write(&self, file: &mut std::fs::File) -> Result<()> {
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 1024 * 1024,
            "Timeseries receipt exceeds cleanup read budget"
        );
        let mut receipt: Receipt = serde_json::from_slice(&bytes)?;
        if let Some(journal) = receipt
            .lifecycle
            .as_mut()
            .and_then(|lifecycle| lifecycle.crash.as_mut())
        {
            *journal = self.journal.clone();
        }
        receipt.settle(false);
        persist(file, &receipt)
    }
}
pub(super) fn register(file: &std::fs::File) -> Result<(Arc<Mutex<Handles>>, CleanupRegistration)> {
    let handles = Arc::new(Mutex::new(Handles {
        watch: None,
        closing: None,
        journal: Journal::default(),
        end: None,
    }));
    let retained = handles.clone();
    let mut writer = file.try_clone()?;
    let registration = owner::register_cleanup(
        CleanupKind::Remote,
        "Timeseries crash watch",
        &uuid::Uuid::now_v7().to_string(),
        move || async move { retained.lock().await.close(&mut writer).await },
    )?;
    Ok((handles, registration))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(target: &veoveo_deploy_contract::InstallationTarget) -> Result<Fixture> {
        Ok(Fixture {
            schema: Schema::V1,
            context: target.kubernetes.context.clone(),
            namespace: target.kubernetes.namespace.clone(),
            operator_resource: ProtectedResourceId::parse(
                "https://conformance.veoveo.local/mcp/operator",
            )?,
            target: CrashTarget {
                deployment: "timeseries-mcp".into(),
                pod: "timeseries-mcp-selected".into(),
                container: "timeseries-mcp".into(),
                namespace_uid: uuid::Uuid::now_v7(),
                deployment_uid: uuid::Uuid::now_v7(),
                replica_set_uid: uuid::Uuid::now_v7(),
                pod_uid: uuid::Uuid::now_v7(),
                container_id: "containerd://selected-instance".into(),
                image_id: "registry.example/timeseries@sha256:selected".into(),
                restart_count: 0,
            },
        })
    }
    #[test]
    fn timeseries_crash_fixture_rejects_foreign_installation_role_and_incompatible_modes()
    -> Result<()> {
        let target = veoveo_deploy_contract::InstallationTarget::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../installation-target.json"),
        )?;
        let fixture = fixture(&target)?;
        fixture.admit_installation(&target, &fixture.operator_resource)?;
        let mut other = fixture.clone();
        other.context.push_str("-foreign");
        assert!(
            other
                .admit_installation(&target, &fixture.operator_resource)
                .is_err()
        );
        other = fixture.clone();
        other.namespace.push_str("-foreign");
        assert!(
            other
                .admit_installation(&target, &fixture.operator_resource)
                .is_err()
        );
        assert!(
            fixture
                .admit_installation(
                    &target,
                    &ProtectedResourceId::parse("https://conformance.veoveo.local/mcp/foreign")?
                )
                .is_err()
        );
        other = fixture.clone();
        other.target.container = "worker-sidecar".into();
        assert!(other.validate().is_err());
        other = fixture.clone();
        other.target.deployment = "undeclared-workload".into();
        assert!(
            other
                .admit_installation(&target, &fixture.operator_resource)
                .is_err()
        );
        other = fixture.clone();
        other.target.pod_uid = uuid::Uuid::nil();
        assert!(other.validate().is_err());
        let mut wire = serde_json::to_value(&fixture)?;
        wire["unknown"] = true.into();
        assert!(serde_json::from_value::<Fixture>(wire).is_err());
        let mut input = Input {
            mode: Mode::ProcessCrash,
            request: forecast_request()?,
            crash: Some(fixture),
        };
        input.validate()?;
        input.mode = Mode::Cancel;
        assert!(input.validate().is_err());
        input.mode = Mode::Reconnect;
        assert!(input.validate().is_err());
        input.mode = Mode::ProcessCrash;
        input.crash = None;
        assert!(input.validate().is_err());
        Ok(())
    }
    #[tokio::test]
    async fn timeseries_crash_failed_consuming_close_is_sticky_and_retained() -> Result<()> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let polls = Arc::new(AtomicUsize::new(0));
        let observed = polls.clone();
        let mut handles = Handles {
            watch: None,
            closing: Some(Box::pin(async move {
                observed.fetch_add(1, Ordering::SeqCst);
                false
            })),
            journal: Journal {
                close: Close::Pending,
                ..Default::default()
            },
            end: Some(tokio::time::Instant::now() + Duration::from_secs(1)),
        };
        let mut receipt = Receipt::new(forecast_request()?);
        receipt.lifecycle = Some(super::super::Journal {
            input: Input {
                mode: Mode::Cancel,
                request: forecast_request()?,
                crash: None,
            },
            task_id: None,
            created_at: None,
            observations: Vec::new(),
            crash: Some(Journal::default()),
            cleanup: cleanup::Trace::default(),
            output: None,
            artifact_digest: None,
            subscription_closed: false,
            connection_closed: false,
            passed: false,
        });
        let mut file = tempfile::tempfile()?;
        persist(&mut file, &receipt)?;
        assert!(handles.close(&mut file).await.is_err());
        assert!(handles.close(&mut file).await.is_err());
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(!handles.closed() && handles.closing.is_some());
        file.seek(SeekFrom::Start(0))?;
        let saved: Receipt = serde_json::from_reader(file)?;
        assert!(saved.lifecycle.unwrap().crash.unwrap().close == Close::Failed);
        let polls = Arc::new(AtomicUsize::new(0));
        let observed = polls.clone();
        handles.closing = Some(Box::pin(async move {
            observed.fetch_add(1, Ordering::SeqCst);
            true
        }));
        handles.journal.close = Close::Pending;
        handles.end = Some(tokio::time::Instant::now());
        let mut file = tempfile::tempfile()?;
        persist(&mut file, &receipt)?;
        assert!(handles.close(&mut file).await.is_err());
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert!(!handles.closed());
        Ok(())
    }
    #[tokio::test]
    async fn timeseries_crash_receipt_failure_still_closes_actual_owned_process() -> Result<()> {
        const MODE: &str = "VEOVEO_TIMESERIES_CRASH_IO_CONTROL";
        if std::env::var_os(MODE).is_some() {
            return owner::run(async {
                use veoveo_testing_support::process::spawn_async;
                let mut command = tokio::process::Command::new("sh");
                command.args(["-c", "exec sleep 60"]);
                let mut child = spawn_async(command)?;
                let end = owner::cleanup_deadline()?;
                let settled = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let observed = settled.clone();
                let mut handles = Handles {
                    watch: None,
                    closing: Some(Box::pin(async move {
                        let result = child.cleanup_until(end).await;
                        observed.store(result.is_ok(), std::sync::atomic::Ordering::SeqCst);
                        result.is_ok()
                    })),
                    journal: Journal {
                        close: Close::Pending,
                        ..Default::default()
                    },
                    end: Some(tokio::time::Instant::from_std(end)),
                };
                // An invalid receipt makes both journal attempts fail before/after close.
                let mut file = tempfile::tempfile()?;
                std::io::Write::write_all(&mut file, b"invalid receipt")?;
                assert!(handles.close(&mut file).await.is_err());
                assert!(settled.load(std::sync::atomic::Ordering::SeqCst));
                assert!(handles.closed() && handles.journal.journal_write_failed);
                // Repairing the file cannot erase the recorded persistence failure.
                persist(&mut file, &Receipt::new(forecast_request()?))?;
                assert!(handles.close(&mut file).await.is_err());
                std::fs::write(
                    PathBuf::from(
                        std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS")
                            .context("native control directory absent")?,
                    )
                    .join("observed"),
                    b"closed",
                )?;
                Ok(())
            })
            .await;
        }
        let directory = tempfile::tempdir()?;
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            + 20_000;
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command.args(["--exact", "timeseries::lifecycle::recovery::tests::timeseries_crash_receipt_failure_still_closes_actual_owned_process", "--nocapture"])
            .env(MODE, "child")
            .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.to_string())
            .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "2")
            .env("VEOVEO_SMOKE_LOCAL_GROUPS", directory.path());
        let output = veoveo_testing_support::output_async(command, Duration::from_secs(15)).await?;
        ensure!(
            output.status.success(),
            "Timeseries journal failure native cleanup control failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        ensure!(
            std::fs::read(directory.path().join("observed"))? == b"closed",
            "Timeseries native cleanup control did not execute"
        );
        Ok(())
    }
    #[tokio::test]
    async fn timeseries_crash_refuses_ready_marker_after_original_instance_fence_failure()
    -> Result<()> {
        let target = veoveo_deploy_contract::InstallationTarget::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../installation-target.json"),
        )?;
        let input = Input {
            mode: Mode::ProcessCrash,
            request: forecast_request()?,
            crash: Some(fixture(&target)?),
        };
        let mut receipt = Receipt::new(forecast_request()?);
        receipt.lifecycle = Some(super::super::Journal {
            input,
            task_id: Some(CanonicalTaskId::parse("gateway-original-forecast")?),
            created_at: Some("2026-10-10T00:00:00Z".into()),
            observations: Vec::new(),
            crash: Some(Journal::default()),
            cleanup: cleanup::Trace::default(),
            output: None,
            artifact_digest: None,
            subscription_closed: false,
            connection_closed: false,
            passed: false,
        });
        let mut file = tempfile::tempfile()?;
        persist(&mut file, &receipt)?;
        // The shared original CrashTarget admission rejects a changed container/Pod;
        // this owner seam must propagate that rejection before publishing the marker.
        let revalidation = async { bail!("live crash target differs from admitted fixture") };
        assert!(
            ready_for_crash(revalidation, &mut file, &mut receipt)
                .await
                .is_err()
        );
        assert!(
            !receipt
                .lifecycle
                .as_ref()
                .unwrap()
                .observations
                .iter()
                .any(|observation| observation.step == Step::ReadyForCrash)
        );
        file.seek(SeekFrom::Start(0))?;
        let saved: Receipt = serde_json::from_reader(&mut file)?;
        assert!(saved.lifecycle.unwrap().observations.is_empty());
        ready_for_crash(async { Ok(()) }, &mut file, &mut receipt).await?;
        assert!(
            receipt.lifecycle.unwrap().observations.last().unwrap().step == Step::ReadyForCrash
        );
        Ok(())
    }
}
