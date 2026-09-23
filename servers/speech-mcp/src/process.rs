//! The Speech service owns one persistent CUDA worker and its private workspace.
use crate::worker::{PROTOCOL, WorkerConnection, WorkerEvent, WorkerRequest};
use anyhow::{Result, bail, ensure};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    sync::Mutex,
    time::{Instant, sleep},
};

pub struct WorkerProcess {
    child: Mutex<Child>,
    workspace: tempfile::TempDir,
    socket: PathBuf,
}

impl WorkerProcess {
    pub async fn start(python: &Path, capacity: u8) -> Result<Self> {
        ensure!(
            (1..=8).contains(&capacity),
            "speech capacity must be between one and eight"
        );
        let workspace = tempfile::Builder::new()
            .prefix("veoveo-speech-")
            .tempdir()?;
        let socket = workspace.path().join("worker.sock");
        let mut child = Command::new(python)
            .arg("-m")
            .arg("speech_runner.main")
            .arg("--socket")
            .arg(&socket)
            .arg("--work-dir")
            .arg(workspace.path())
            .arg("--capacity")
            .arg(capacity.to_string())
            .env("HF_HUB_OFFLINE", "1")
            .env_remove("MOONDREAM_API_KEY")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(90);
        while !socket.exists() {
            if let Some(status) = child.try_wait()? {
                bail!("speech worker exited before readiness: {status}");
            }
            ensure!(
                Instant::now() < deadline,
                "speech worker readiness timed out"
            );
            sleep(Duration::from_millis(100)).await;
        }
        let process = Self {
            child: Mutex::new(child),
            workspace,
            socket,
        };
        process.ready().await?;
        Ok(process)
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }
    pub fn workspace(&self) -> &Path {
        self.workspace.path()
    }

    pub async fn ready(&self) -> Result<()> {
        ensure!(
            self.child.lock().await.try_wait()?.is_none(),
            "speech worker stopped"
        );
        let mut probe = WorkerConnection::connect(&self.socket, &WorkerRequest::Probe).await?;
        let ready = tokio::time::timeout(Duration::from_secs(5), probe.event()).await??;
        match ready {
            WorkerEvent::Ready {
                protocol,
                device,
                model,
                revision,
            } => {
                ensure!(
                    protocol == PROTOCOL && device.starts_with("cuda:NVIDIA"),
                    "hardware speech worker required"
                );
                ensure!(
                    model == crate::model::MODEL && revision == crate::model::MODEL_REVISION,
                    "speech worker model identity differs from the installed contract"
                );
                Ok(())
            }
            _ => bail!("speech worker did not establish readiness"),
        }
    }
}
