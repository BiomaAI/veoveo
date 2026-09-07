use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use veoveo_extension_contract::ArtifactDigest;

/// Hashes the actual chart files in a verified immutable source checkout.
///
/// The source resolver owns revision verification and must preserve the checkout
/// through rendering. Git archive headers, mtimes, and export attributes do not
/// participate. Symlinks and special files cannot extend the declared input tree.
pub fn source_chart_content_digest(repository: &Path, chart: &Path) -> Result<ArtifactDigest> {
    ensure!(
        !chart.as_os_str().is_empty()
            && chart
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && chart.to_str().is_some_and(|path| path
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")
                && !path.contains('\\')),
        "source chart must be a canonical repository-relative path"
    );
    let mut root = repository.to_path_buf();
    for part in chart.components() {
        root.push(part);
        let metadata = fs::symlink_metadata(&root).context("reading source chart directory")?;
        ensure!(
            metadata.is_dir(),
            "source chart path must contain only real directories"
        );
    }
    let mut files = Vec::new();
    collect_files(&root, &root, &mut files)?;
    files.sort();
    ensure!(
        files.iter().any(|path| path == Path::new("Chart.yaml")),
        "source chart must contain Chart.yaml"
    );
    let mut hash = Sha256::new();
    hash.update(b"veoveo.io/source-chart-content/v1\0");
    for relative in files {
        let name = relative
            .to_str()
            .context("source chart paths must be UTF-8")?;
        hash.update(u64::try_from(name.len())?.to_be_bytes());
        hash.update(name.as_bytes());
        let path = root.join(relative);
        let metadata = fs::symlink_metadata(&path).context("reading source chart file metadata")?;
        ensure!(
            metadata.is_file(),
            "source chart entry changed type during hashing"
        );
        hash.update([executable(&metadata)?]);
        hash.update(metadata.len().to_be_bytes());
        let mut file = fs::File::open(&path).context("opening source chart file")?;
        let mut buffer = [0_u8; 64 * 1024];
        let mut length = 0_u64;
        loop {
            let count = file
                .read(&mut buffer)
                .context("reading source chart file")?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
            length += u64::try_from(count)?;
        }
        ensure!(
            length == metadata.len(),
            "source chart file changed length during hashing"
        );
    }
    let hex = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(ArtifactDigest::new(format!("sha256:{hex}"))?)
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory).context("reading source chart entries")? {
        let entry = entry.context("reading source chart entry")?;
        let kind = entry
            .file_type()
            .context("reading source chart entry type")?;
        let path = entry.path();
        if kind.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            ensure!(
                kind.is_file(),
                "source chart cannot contain symlinks or special files"
            );
            files.push(path.strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn executable(metadata: &fs::Metadata) -> Result<u8> {
    use std::os::unix::fs::PermissionsExt;
    Ok(u8::from(metadata.permissions().mode() & 0o111 != 0))
}

#[cfg(not(unix))]
fn executable(_metadata: &fs::Metadata) -> Result<u8> {
    anyhow::bail!("source chart hashing requires a filesystem that retains Git executable modes")
}
