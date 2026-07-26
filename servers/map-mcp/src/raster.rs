use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use tokio_util::sync::CancellationToken;

use crate::contract::RasterDerivationOperation;

const OPERATION_SCHEMA_VERSION: u64 = 1;
const MAX_PROTOCOL_BYTES: u64 = 1_048_576;

#[derive(Clone, Debug)]
pub struct RasterServiceConfig {
    pub python_executable: PathBuf,
    pub module: String,
    pub maximum_output_bytes: u64,
    pub timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct RasterService {
    config: RasterServiceConfig,
}

#[derive(Debug, Serialize)]
struct RasterCommand<'a> {
    schema_version: u64,
    source_path: &'a Path,
    output_dir: &'a Path,
    maximum_output_bytes: u64,
    operation: &'a RasterDerivationOperation,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedRasterDerivation {
    pub path: PathBuf,
    pub filename: String,
    pub mime_type: String,
    pub output_crs: String,
    pub output_transform: Option<[f64; 6]>,
}

impl RasterService {
    pub fn new(config: RasterServiceConfig) -> Result<Self> {
        if !config.python_executable.is_absolute()
            || config.module.is_empty()
            || config.module.len() > 128
            || !config
                .module
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
            || config.maximum_output_bytes == 0
            || config.timeout.is_zero()
        {
            bail!("invalid raster service configuration");
        }
        Ok(Self { config })
    }

    pub async fn derive(
        &self,
        source_path: &Path,
        output_dir: &Path,
        operation: &RasterDerivationOperation,
        cancellation: CancellationToken,
    ) -> Result<GeneratedRasterDerivation> {
        if !source_path.is_absolute() || !source_path.is_file() || !output_dir.is_absolute() {
            bail!("raster operation paths must be absolute and the source must exist");
        }
        tokio::fs::create_dir_all(output_dir).await?;
        let input = serde_json::to_vec(&RasterCommand {
            schema_version: OPERATION_SCHEMA_VERSION,
            source_path,
            output_dir,
            maximum_output_bytes: self.config.maximum_output_bytes,
            operation,
        })?;
        let mut process = Command::new(&self.config.python_executable);
        process
            .arg("-m")
            .arg(&self.config.module)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        process.process_group(0);
        let mut child = process.spawn().context("starting raster operation")?;
        let pid = child.id().context("raster operation has no process id")?;
        child
            .stdin
            .take()
            .context("raster operation stdin missing")?
            .write_all(&input)
            .await?;
        let stdout = child.stdout.take().context("raster stdout missing")?;
        let stderr = child.stderr.take().context("raster stderr missing")?;
        let read_stdout = tokio::spawn(read_bounded(stdout, MAX_PROTOCOL_BYTES));
        let read_stderr = tokio::spawn(read_bounded(stderr, MAX_PROTOCOL_BYTES));
        let status = tokio::select! {
            status = tokio::time::timeout(self.config.timeout, child.wait()) => {
                match status {
                    Ok(status) => status?,
                    Err(_) => {
                        terminate_process_group(pid).await;
                        bail!("raster operation exceeded its time limit");
                    }
                }
            }
            () = cancellation.cancelled() => {
                terminate_process_group(pid).await;
                bail!("raster operation was cancelled");
            }
        };
        let (stdout, stderr) = tokio::try_join!(read_stdout, read_stderr)?;
        let stdout = stdout?;
        let stderr = stderr?;
        if !status.success() {
            bail!(
                "raster operation failed: {}",
                String::from_utf8_lossy(&stderr)
                    .chars()
                    .take(4_096)
                    .collect::<String>()
            );
        }
        let generated: GeneratedRasterDerivation =
            serde_json::from_slice(&stdout).context("decoding raster operation result")?;
        validate_generated(&generated, output_dir, self.config.maximum_output_bytes)?;
        Ok(generated)
    }
}

async fn read_bounded<R: tokio::io::AsyncRead + Unpin>(reader: R, limit: u64) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    reader.take(limit + 1).read_to_end(&mut output).await?;
    if output.len() as u64 > limit {
        bail!("raster helper exceeded its protocol byte limit");
    }
    Ok(output)
}

fn validate_generated(
    generated: &GeneratedRasterDerivation,
    output_dir: &Path,
    maximum_output_bytes: u64,
) -> Result<()> {
    let root = output_dir.canonicalize()?;
    let path = generated.path.canonicalize()?;
    let metadata = path.metadata()?;
    if !path.starts_with(root)
        || !metadata.is_file()
        || metadata.len() > maximum_output_bytes
        || generated.filename.is_empty()
        || generated.filename.len() > 256
        || generated.filename.contains('/')
        || generated.filename.contains('\\')
        || generated.mime_type.is_empty()
        || generated.mime_type.len() > 256
        || generated.mime_type.chars().any(char::is_control)
        || generated.output_crs.is_empty()
        || generated.output_crs.len() > 16_384
        || generated.output_crs.chars().any(char::is_control)
        || generated.output_transform.is_some_and(|transform| {
            transform.iter().any(|value| !value.is_finite())
                || transform[1] * transform[5] - transform[2] * transform[4] == 0.0
        })
    {
        bail!("raster helper returned an invalid or unconfined product");
    }
    Ok(())
}

async fn terminate_process_group(pid: u32) {
    let group = Pid::from_raw(pid as i32);
    let _ = killpg(group, Signal::SIGTERM);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = killpg(group, Signal::SIGKILL);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_service_requires_an_absolute_python_path() {
        assert!(
            RasterService::new(RasterServiceConfig {
                python_executable: PathBuf::from("python3"),
                module: "map_data.raster_ops".to_owned(),
                maximum_output_bytes: 1024,
                timeout: Duration::from_secs(1),
            })
            .is_err()
        );
    }
}
