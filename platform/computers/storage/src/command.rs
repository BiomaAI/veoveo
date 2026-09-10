//! Bounded calls to the selected compute host's filesystem tools.
use crate::{Result, StorageError};
use std::{
    ffi::OsStr,
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

pub(crate) struct Output {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr_empty: bool,
}

async fn bounded_read(stream: impl AsyncRead + Unpin) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream
        .take(8193)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| StorageError::BackendUnavailable)?;
    if bytes.len() > 8192 {
        return Err(StorageError::BackendUnavailable);
    }
    Ok(bytes)
}

pub(crate) async fn run(program: &str, args: &[&OsStr]) -> Result<Output> {
    tokio::time::timeout(Duration::from_secs(90), async {
        let mut child = Command::new(program)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| StorageError::BackendUnavailable)?;
        let stdout = child
            .stdout
            .take()
            .ok_or(StorageError::BackendUnavailable)?;
        let stderr = child
            .stderr
            .take()
            .ok_or(StorageError::BackendUnavailable)?;
        let (stdout, stderr, status) =
            tokio::try_join!(bounded_read(stdout), bounded_read(stderr), async {
                child
                    .wait()
                    .await
                    .map_err(|_| StorageError::BackendUnavailable)
            })?;
        Ok(Output {
            status,
            stdout: String::from_utf8(stdout).map_err(|_| StorageError::BackendUnavailable)?,
            stderr_empty: stderr.is_empty(),
        })
    })
    .await
    .map_err(|_| StorageError::BackendUnavailable)?
}

pub(crate) async fn checked(program: &str, args: &[&OsStr]) -> Result<String> {
    let output = run(program, args).await?;
    if !output.status.success() {
        return Err(StorageError::BackendUnavailable);
    }
    Ok(output.stdout.trim().to_owned())
}
