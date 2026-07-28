use std::{
    env,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use flate2::read::GzDecoder;
use futures::StreamExt;
use reqwest::redirect::Policy;
use sha2::{Digest, Sha256};

const DUCKDB_VERSION: &str = "1.5.5";
const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_EXTENSION_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy)]
struct SpatialArtifact {
    platform: &'static str,
    archive_sha256: &'static str,
    extension_sha256: &'static str,
}

impl SpatialArtifact {
    fn current() -> Result<Self> {
        match env::consts::ARCH {
            "x86_64" => Ok(Self {
                platform: "linux_amd64",
                archive_sha256: "832edb1b189d53281baf552034028a4dfe317c5381a7e7d2d9de74a4e875572e",
                extension_sha256: "03cbb687fbb1583af6154266dc17b9c99e11fdcc1cd4a43cec19f38ef272d4de",
            }),
            "aarch64" => Ok(Self {
                platform: "linux_arm64",
                archive_sha256: "ccf86767b4f28471963e0950cb6a9d9bf2e38f6f9a476de32f5b59ffdacfc239",
                extension_sha256: "b9129a7da7fde8eb7fb951d99a115cbf2e6fa34c8d41671086169174a1f5725d",
            }),
            architecture => {
                bail!("the native DuckDB smoke does not support architecture `{architecture}`")
            }
        }
    }

    fn url(self) -> String {
        format!(
            "https://extensions.duckdb.org/v{DUCKDB_VERSION}/{}/spatial.duckdb_extension.gz",
            self.platform
        )
    }
}

pub(crate) async fn provision_duckdb_spatial_extension() -> Result<PathBuf> {
    let artifact = SpatialArtifact::current()?;
    let cache_directory = cargo_target_directory()?
        .join("smoke-assets/duckdb")
        .join(format!("v{DUCKDB_VERSION}"))
        .join(artifact.platform);
    fs::create_dir_all(&cache_directory)
        .with_context(|| format!("creating {}", cache_directory.display()))?;
    let extension = cache_directory.join("spatial.duckdb_extension");

    if extension.is_file() && sha256_file(&extension)? == artifact.extension_sha256 {
        return Ok(extension);
    }

    println!(
        "fetching pinned DuckDB Spatial v{DUCKDB_VERSION} for {}",
        artifact.platform
    );
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .build()
        .context("building DuckDB Spatial download client")?;
    let response = client
        .get(artifact.url())
        .send()
        .await
        .context("downloading pinned DuckDB Spatial archive")?
        .error_for_status()
        .context("DuckDB Spatial archive returned an error status")?;
    if let Some(length) = response.content_length() {
        ensure!(
            length <= MAX_ARCHIVE_BYTES as u64,
            "DuckDB Spatial archive declares {length} bytes, above the {MAX_ARCHIVE_BYTES}-byte limit"
        );
    }

    let mut archive = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("reading DuckDB Spatial archive")?;
        ensure!(
            archive.len() + chunk.len() <= MAX_ARCHIVE_BYTES,
            "DuckDB Spatial archive exceeded the {MAX_ARCHIVE_BYTES}-byte limit"
        );
        archive.extend_from_slice(&chunk);
    }
    ensure!(
        sha256_bytes(&archive) == artifact.archive_sha256,
        "DuckDB Spatial archive digest does not match the repository pin"
    );

    let mut temporary = tempfile::NamedTempFile::new_in(&cache_directory).with_context(|| {
        format!(
            "creating DuckDB Spatial temporary file in {}",
            cache_directory.display()
        )
    })?;
    let decoder = GzDecoder::new(archive.as_slice());
    let written = io::copy(
        &mut decoder.take(MAX_EXTENSION_BYTES + 1),
        temporary.as_file_mut(),
    )
    .context("decompressing DuckDB Spatial extension")?;
    ensure!(
        written <= MAX_EXTENSION_BYTES,
        "DuckDB Spatial extension exceeded the {MAX_EXTENSION_BYTES}-byte limit"
    );
    temporary
        .as_file()
        .sync_all()
        .context("syncing DuckDB Spatial extension")?;
    ensure!(
        sha256_file(temporary.path())? == artifact.extension_sha256,
        "decompressed DuckDB Spatial extension digest does not match the repository pin"
    );

    if extension.exists() {
        fs::remove_file(&extension)
            .with_context(|| format!("replacing invalid cache file {}", extension.display()))?;
    }
    temporary
        .persist(&extension)
        .map_err(|error| error.error)
        .with_context(|| format!("installing {}", extension.display()))?;
    Ok(extension)
}

fn cargo_target_directory() -> Result<PathBuf> {
    let target = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"));
    if target.is_absolute() {
        Ok(target)
    } else {
        Ok(env::current_dir()
            .context("resolving the current directory for the Cargo target")?
            .join(target))
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("reading {}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_and_native_smoke_share_spatial_archive_pins() {
        let dockerfile = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../servers/duckdb-mcp/Dockerfile"),
        )
        .unwrap();
        assert!(dockerfile.contains("ARG DUCKDB_VERSION=1.5.5"));
        for artifact in [
            SpatialArtifact {
                platform: "linux_amd64",
                archive_sha256: "832edb1b189d53281baf552034028a4dfe317c5381a7e7d2d9de74a4e875572e",
                extension_sha256: "03cbb687fbb1583af6154266dc17b9c99e11fdcc1cd4a43cec19f38ef272d4de",
            },
            SpatialArtifact {
                platform: "linux_arm64",
                archive_sha256: "ccf86767b4f28471963e0950cb6a9d9bf2e38f6f9a476de32f5b59ffdacfc239",
                extension_sha256: "b9129a7da7fde8eb7fb951d99a115cbf2e6fa34c8d41671086169174a1f5725d",
            },
        ] {
            assert!(dockerfile.contains(artifact.platform));
            assert!(dockerfile.contains(artifact.archive_sha256));
        }
    }

    #[test]
    fn cached_extension_digest_is_verified() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("spatial.duckdb_extension");
        fs::write(&path, b"pinned extension").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            sha256_bytes(b"pinned extension")
        );
    }
}
