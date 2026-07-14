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

use crate::contract::{AcquisitionId, SourceAdapterKind};

const HELPER_SCHEMA_VERSION: u32 = 1;
const MAX_HELPER_OUTPUT_BYTES: u64 = 1_048_576;
const MAX_HELPER_DIAGNOSTIC_BYTES: u64 = 1_048_576;

#[derive(Clone, Debug)]
pub struct AcquisitionHelperConfig {
    pub python_executable: PathBuf,
    pub module: String,
    pub maximum_output_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct AcquisitionHelper {
    config: AcquisitionHelperConfig,
}

#[derive(Clone, Debug, Serialize)]
struct NormalizeCommand {
    schema_version: u32,
    acquisition_id: String,
    adapter_kind: SourceAdapterKind,
    source_path: PathBuf,
    output_dir: PathBuf,
    maximum_elapsed_seconds: u64,
    maximum_output_bytes: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NormalizeResult {
    pub schema_version: u32,
    pub acquisition_id: AcquisitionId,
    pub source_digest_sha256: String,
    pub version_label: String,
    pub normalized_paths: Vec<PathBuf>,
    pub quality_report_path: PathBuf,
    pub routing_build_path: Option<PathBuf>,
}

impl AcquisitionHelper {
    pub fn new(config: AcquisitionHelperConfig) -> Result<Self> {
        if !config.python_executable.is_absolute()
            || config.module.is_empty()
            || config.module.len() > 128
            || !config
                .module
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.')
            || config.maximum_output_bytes == 0
        {
            bail!("invalid acquisition-helper configuration");
        }
        Ok(Self { config })
    }

    pub async fn normalize(
        &self,
        acquisition_id: &AcquisitionId,
        adapter_kind: SourceAdapterKind,
        source_path: &Path,
        output_dir: &Path,
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<NormalizeResult> {
        if !source_path.is_absolute() || !output_dir.is_absolute() || timeout.is_zero() {
            bail!("helper paths must be absolute and timeout must be positive");
        }
        tokio::fs::create_dir_all(output_dir).await?;
        let command = NormalizeCommand {
            schema_version: HELPER_SCHEMA_VERSION,
            acquisition_id: acquisition_id.to_string(),
            adapter_kind,
            source_path: source_path.to_owned(),
            output_dir: output_dir.to_owned(),
            maximum_elapsed_seconds: timeout.as_secs().max(1),
            maximum_output_bytes: self.config.maximum_output_bytes,
        };
        let input = serde_json::to_vec(&command)?;
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
        let mut child = process.spawn().context("starting map acquisition helper")?;
        let pid = child.id().context("acquisition helper has no process id")?;
        child
            .stdin
            .take()
            .context("acquisition helper stdin missing")?
            .write_all(&input)
            .await?;
        let stdout = child.stdout.take().context("helper stdout missing")?;
        let stderr = child.stderr.take().context("helper stderr missing")?;
        let read_stdout = tokio::spawn(read_bounded(stdout, MAX_HELPER_OUTPUT_BYTES));
        let read_stderr = tokio::spawn(read_bounded(stderr, MAX_HELPER_DIAGNOSTIC_BYTES));
        let status = tokio::select! {
            status = tokio::time::timeout(timeout, child.wait()) => {
                match status {
                    Ok(status) => status?,
                    Err(_) => {
                        terminate_process_group(pid).await;
                        bail!("map acquisition helper exceeded its time limit");
                    }
                }
            }
            () = cancellation.cancelled() => {
                terminate_process_group(pid).await;
                bail!("map acquisition helper was cancelled");
            }
        };
        let (stdout, stderr) = tokio::try_join!(read_stdout, read_stderr)?;
        let stdout = stdout?;
        let stderr = stderr?;
        if !status.success() {
            let diagnostic = String::from_utf8_lossy(&stderr);
            bail!(
                "map acquisition helper failed: {}",
                diagnostic.chars().take(4096).collect::<String>()
            );
        }
        let result: NormalizeResult =
            serde_json::from_slice(&stdout).context("decoding acquisition helper result")?;
        validate_result(&result, acquisition_id, output_dir)?;
        Ok(result)
    }
}

async fn read_bounded<R: tokio::io::AsyncRead + Unpin>(reader: R, limit: u64) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    reader.take(limit + 1).read_to_end(&mut output).await?;
    if output.len() as u64 > limit {
        bail!("acquisition helper exceeded its output limit");
    }
    Ok(output)
}

async fn terminate_process_group(pid: u32) {
    let group = Pid::from_raw(pid as i32);
    let _ = killpg(group, Signal::SIGTERM);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = killpg(group, Signal::SIGKILL);
}

fn validate_result(
    result: &NormalizeResult,
    acquisition_id: &AcquisitionId,
    output_dir: &Path,
) -> Result<()> {
    if result.schema_version != HELPER_SCHEMA_VERSION || result.acquisition_id != *acquisition_id {
        bail!("acquisition helper returned a mismatched contract identity");
    }
    if result.source_digest_sha256.len() != 64
        || !result
            .source_digest_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || result.version_label.is_empty()
        || result.version_label.len() > 256
    {
        bail!("acquisition helper returned invalid release metadata");
    }
    let root = output_dir.canonicalize()?;
    if result.normalized_paths.is_empty() {
        bail!("acquisition helper returned no normalized products");
    }
    for path in result
        .normalized_paths
        .iter()
        .chain(std::iter::once(&result.quality_report_path))
        .chain(result.routing_build_path.iter())
    {
        let canonical = path.canonicalize()?;
        if !canonical.starts_with(&root) || !canonical.is_file() {
            bail!("acquisition helper returned an unconfined or non-file product");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_configuration_requires_an_absolute_python_path() {
        assert!(
            AcquisitionHelper::new(AcquisitionHelperConfig {
                python_executable: PathBuf::from("python3"),
                module: "map_data".to_owned(),
                maximum_output_bytes: 1024,
            })
            .is_err()
        );
    }
}
