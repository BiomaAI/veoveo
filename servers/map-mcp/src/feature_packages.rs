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
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use tokio_util::sync::CancellationToken;

use crate::contract::{GeoPackageIdentifier, GeoPackageManifest, MAX_IMPORT_FEATURES};

const SCHEMA_VERSION: u32 = 1;
const MAX_PROTOCOL_BYTES: u64 = 16 * 1_048_576;
const MAX_DIAGNOSTIC_BYTES: u64 = 1_048_576;

#[derive(Clone, Debug)]
pub struct FeaturePackageServiceConfig {
    pub python_executable: PathBuf,
    pub module: String,
    pub maximum_output_bytes: u64,
    pub timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct FeaturePackageService {
    config: FeaturePackageServiceConfig,
}

#[derive(Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum FeaturePackageCommand<'a> {
    Inspect {
        schema_version: u32,
        source_path: &'a Path,
    },
    Decode {
        schema_version: u32,
        source_path: &'a Path,
        output_dir: &'a Path,
        maximum_output_bytes: u64,
        maximum_features: usize,
        table: &'a GeoPackageIdentifier,
        identity_column: Option<&'a GeoPackageIdentifier>,
        semantic_type_column: Option<&'a GeoPackageIdentifier>,
        default_semantic_type: &'a str,
        title_column: Option<&'a GeoPackageIdentifier>,
        valid_from_column: Option<&'a GeoPackageIdentifier>,
        valid_until_column: Option<&'a GeoPackageIdentifier>,
    },
    Encode {
        schema_version: u32,
        source_path: &'a Path,
        output_dir: &'a Path,
        maximum_output_bytes: u64,
        table: &'a GeoPackageIdentifier,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InspectResult {
    schema_version: u32,
    manifest: GeoPackageManifest,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedFeaturePackageFile {
    schema_version: u32,
    pub path: PathBuf,
    pub filename: String,
    pub mime_type: String,
    pub feature_count: u64,
    pub byte_count: u64,
    pub digest_sha256: String,
}

#[derive(Debug)]
pub struct GeoPackageDecode<'a> {
    pub table: &'a GeoPackageIdentifier,
    pub identity_column: Option<&'a GeoPackageIdentifier>,
    pub semantic_type_column: Option<&'a GeoPackageIdentifier>,
    pub default_semantic_type: &'a str,
    pub title_column: Option<&'a GeoPackageIdentifier>,
    pub valid_from_column: Option<&'a GeoPackageIdentifier>,
    pub valid_until_column: Option<&'a GeoPackageIdentifier>,
}

impl FeaturePackageService {
    pub fn new(config: FeaturePackageServiceConfig) -> Result<Self> {
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
            bail!("invalid feature-package service configuration");
        }
        Ok(Self { config })
    }

    pub async fn inspect(
        &self,
        source_path: &Path,
        cancellation: CancellationToken,
    ) -> Result<GeoPackageManifest> {
        validate_source(source_path)?;
        let result: InspectResult = self
            .run(
                &FeaturePackageCommand::Inspect {
                    schema_version: SCHEMA_VERSION,
                    source_path,
                },
                cancellation,
            )
            .await?;
        if result.schema_version != SCHEMA_VERSION {
            bail!("feature-package helper returned a mismatched schema version");
        }
        Ok(result.manifest)
    }

    pub async fn decode(
        &self,
        source_path: &Path,
        output_dir: &Path,
        request: GeoPackageDecode<'_>,
        cancellation: CancellationToken,
    ) -> Result<GeneratedFeaturePackageFile> {
        validate_source(source_path)?;
        validate_output_directory(output_dir)?;
        let result = self
            .run(
                &FeaturePackageCommand::Decode {
                    schema_version: SCHEMA_VERSION,
                    source_path,
                    output_dir,
                    maximum_output_bytes: self.config.maximum_output_bytes,
                    maximum_features: MAX_IMPORT_FEATURES,
                    table: request.table,
                    identity_column: request.identity_column,
                    semantic_type_column: request.semantic_type_column,
                    default_semantic_type: request.default_semantic_type,
                    title_column: request.title_column,
                    valid_from_column: request.valid_from_column,
                    valid_until_column: request.valid_until_column,
                },
                cancellation,
            )
            .await?;
        validate_generated(
            &result,
            output_dir,
            self.config.maximum_output_bytes,
            "application/geo+json-seq",
        )?;
        Ok(result)
    }

    pub async fn encode(
        &self,
        source_path: &Path,
        output_dir: &Path,
        table: &GeoPackageIdentifier,
        cancellation: CancellationToken,
    ) -> Result<GeneratedFeaturePackageFile> {
        validate_source(source_path)?;
        validate_output_directory(output_dir)?;
        let result = self
            .run(
                &FeaturePackageCommand::Encode {
                    schema_version: SCHEMA_VERSION,
                    source_path,
                    output_dir,
                    maximum_output_bytes: self.config.maximum_output_bytes,
                    table,
                },
                cancellation,
            )
            .await?;
        validate_generated(
            &result,
            output_dir,
            self.config.maximum_output_bytes,
            "application/geopackage+sqlite3",
        )?;
        Ok(result)
    }

    async fn run<T: DeserializeOwned>(
        &self,
        command: &FeaturePackageCommand<'_>,
        cancellation: CancellationToken,
    ) -> Result<T> {
        let input = serde_json::to_vec(command)?;
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
        let mut child = process.spawn().context("starting feature-package helper")?;
        let pid = child
            .id()
            .context("feature-package helper has no process id")?;
        child
            .stdin
            .take()
            .context("feature-package helper stdin missing")?
            .write_all(&input)
            .await?;
        let stdout = child
            .stdout
            .take()
            .context("feature-package helper stdout missing")?;
        let stderr = child
            .stderr
            .take()
            .context("feature-package helper stderr missing")?;
        let read_stdout = tokio::spawn(read_bounded(stdout, MAX_PROTOCOL_BYTES));
        let read_stderr = tokio::spawn(read_bounded(stderr, MAX_DIAGNOSTIC_BYTES));
        let status = tokio::select! {
            status = tokio::time::timeout(self.config.timeout, child.wait()) => {
                match status {
                    Ok(status) => status?,
                    Err(_) => {
                        terminate_process_group(pid).await;
                        bail!("feature-package operation exceeded its time limit");
                    }
                }
            }
            () = cancellation.cancelled() => {
                terminate_process_group(pid).await;
                bail!("feature-package operation was cancelled");
            }
        };
        let (stdout, stderr) = tokio::try_join!(read_stdout, read_stderr)?;
        let stdout = stdout?;
        let stderr = stderr?;
        if !status.success() {
            bail!(
                "feature-package operation failed: {}",
                String::from_utf8_lossy(&stderr)
                    .chars()
                    .take(4_096)
                    .collect::<String>()
            );
        }
        serde_json::from_slice(&stdout).context("decoding feature-package helper result")
    }
}

fn validate_source(source_path: &Path) -> Result<()> {
    if !source_path.is_absolute() || !source_path.is_file() || source_path.is_symlink() {
        bail!("feature-package source must be an absolute regular non-symlink file");
    }
    Ok(())
}

fn validate_output_directory(output_dir: &Path) -> Result<()> {
    if !output_dir.is_absolute() || !output_dir.is_dir() || output_dir.is_symlink() {
        bail!("feature-package output must be an existing absolute non-symlink directory");
    }
    Ok(())
}

fn validate_generated(
    generated: &GeneratedFeaturePackageFile,
    output_dir: &Path,
    maximum_output_bytes: u64,
    expected_mime_type: &str,
) -> Result<()> {
    let root = output_dir.canonicalize()?;
    let path = generated.path.canonicalize()?;
    let metadata = path.metadata()?;
    if generated.schema_version != SCHEMA_VERSION
        || !path.starts_with(root)
        || !metadata.is_file()
        || metadata.len() != generated.byte_count
        || metadata.len() > maximum_output_bytes
        || generated.feature_count == 0
        || generated.feature_count > MAX_IMPORT_FEATURES as u64
        || generated.filename.is_empty()
        || generated.filename.len() > 256
        || generated.filename.contains('/')
        || generated.filename.contains('\\')
        || generated.mime_type != expected_mime_type
        || generated.digest_sha256.len() != 64
        || !generated
            .digest_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("feature-package helper returned an invalid or unconfined output");
    }
    Ok(())
}

async fn read_bounded<R: tokio::io::AsyncRead + Unpin>(reader: R, limit: u64) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    reader.take(limit + 1).read_to_end(&mut output).await?;
    if output.len() as u64 > limit {
        bail!("feature-package helper exceeded its protocol byte limit");
    }
    Ok(output)
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
    fn service_requires_an_absolute_python_path_and_positive_bounds() {
        assert!(
            FeaturePackageService::new(FeaturePackageServiceConfig {
                python_executable: PathBuf::from("python3"),
                module: "map_data.feature_package".to_owned(),
                maximum_output_bytes: 1,
                timeout: Duration::from_secs(1),
            })
            .is_err()
        );
        assert!(
            FeaturePackageService::new(FeaturePackageServiceConfig {
                python_executable: PathBuf::from("/usr/bin/python3"),
                module: "map_data.feature_package".to_owned(),
                maximum_output_bytes: 0,
                timeout: Duration::from_secs(1),
            })
            .is_err()
        );
    }
}
