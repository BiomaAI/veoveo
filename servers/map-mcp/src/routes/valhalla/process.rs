use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use tokio::{process::Command, sync::Mutex};

use super::ValhallaClient;

#[derive(Clone, Debug)]
pub struct ValhallaProcessConfig {
    pub executable: PathBuf,
    pub config_file: PathBuf,
    pub concurrency: u16,
    pub startup_timeout: Duration,
}

struct ManagedProcess {
    child: tokio::process::Child,
    pid: u32,
}

#[derive(Clone)]
pub struct ValhallaProcess {
    config: ValhallaProcessConfig,
    client: ValhallaClient,
    process: Arc<Mutex<Option<ManagedProcess>>>,
    operation: Arc<Mutex<()>>,
}

impl ValhallaProcess {
    pub async fn start(config: ValhallaProcessConfig, client: &ValhallaClient) -> Result<Self> {
        validate_config(&config)?;
        let manager = Self {
            config,
            client: client.clone(),
            process: Arc::new(Mutex::new(None)),
            operation: Arc::new(Mutex::new(())),
        };
        manager.restart().await?;
        Ok(manager)
    }

    pub async fn restart(&self) -> Result<()> {
        let _operation = self.operation.lock().await;
        self.stop_inner().await;
        let mut command = Command::new(&self.config.executable);
        command
            .arg(&self.config.config_file)
            .arg(self.config.concurrency.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let child = command.spawn().with_context(|| {
            format!(
                "starting supervised Valhalla process {}",
                self.config.executable.display()
            )
        })?;
        let pid = child.id().context("Valhalla process has no process id")?;
        *self.process.lock().await = Some(ManagedProcess { child, pid });
        let deadline = tokio::time::Instant::now() + self.config.startup_timeout;
        loop {
            if self.exited().await? {
                bail!("Valhalla exited before becoming ready");
            }
            if self.client.health().await.is_ok() {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                self.stop_inner().await;
                bail!("Valhalla did not become ready before its startup deadline");
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    pub async fn exited(&self) -> Result<bool> {
        let mut process = self.process.lock().await;
        match process.as_mut() {
            Some(process) => Ok(process.child.try_wait()?.is_some()),
            None => Ok(true),
        }
    }

    pub async fn stop(&self) {
        let _operation = self.operation.lock().await;
        self.stop_inner().await;
    }

    async fn stop_inner(&self) {
        let Some(mut process) = self.process.lock().await.take() else {
            return;
        };
        let pid = Pid::from_raw(process.pid as i32);
        let _ = killpg(pid, Signal::SIGTERM);
        if tokio::time::timeout(Duration::from_secs(5), process.child.wait())
            .await
            .is_err()
        {
            let _ = killpg(pid, Signal::SIGKILL);
            let _ = process.child.wait().await;
        }
    }
}

fn validate_config(config: &ValhallaProcessConfig) -> Result<()> {
    if !config.executable.is_absolute()
        || !config.config_file.is_absolute()
        || config.concurrency == 0
        || config.startup_timeout.is_zero()
    {
        bail!("invalid Valhalla process configuration");
    }
    if !config.config_file.is_file() {
        bail!(
            "Valhalla configuration does not exist: {}",
            config.config_file.display()
        );
    }
    Ok(())
}
