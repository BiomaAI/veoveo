//! Fingerprint materialized source; Git filters never rewrite a check's identity.
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

use super::{
    cargo_inputs,
    catalog::relative,
    model::{
        CATALOG_DIRECTORY, FileContent, FileInput, INDEX_PATH, InputScope, PLANNER_VERSION,
        RECEIPT_DIRECTORY, SourceInputs,
    },
    storage::digest,
};

const COMMON: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo",
    "AGENTS.md",
    "tools/xtask/src/commands/test_report",
    "tools/xtask/src/main.rs",
    "tools/xtask/src/process.rs",
];

pub(super) fn snapshot(root: &Path, scope: &InputScope) -> Result<SourceInputs> {
    snapshot_with_graph(root, scope, &mut None)
}

/// The caller may share one graph across a single source observation. Check
/// execution uses independent before/after observations to detect input changes.
pub(super) fn snapshot_with_graph(
    root: &Path,
    scope: &InputScope,
    graph: &mut Option<cargo_inputs::Graph>,
) -> Result<SourceInputs> {
    let mut roots = COMMON
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    match scope {
        InputScope::Repository => (),
        InputScope::Cargo {
            packages,
            roots: extra,
        } => {
            if graph.is_none() {
                *graph = Some(cargo_inputs::Graph::load(root)?);
            }
            roots.extend(
                graph
                    .as_ref()
                    .expect("loaded Cargo graph")
                    .roots(root, packages)?,
            );
            roots.extend(extra.iter().cloned());
        }
        InputScope::Console { roots: extra } => roots.extend(extra.iter().cloned()),
    }
    snapshot_with_roots(root, scope, &roots)
}

fn excluded(path: &str) -> bool {
    path == INDEX_PATH
        || path == RECEIPT_DIRECTORY
        || path.starts_with(&format!("{RECEIPT_DIRECTORY}/"))
}

fn matches_root(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

pub(super) fn snapshot_with_roots(
    root: &Path,
    scope: &InputScope,
    roots: &[String],
) -> Result<SourceInputs> {
    let output = crate::process::output(
        "git",
        [
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        Some(root),
    )?;
    let mut paths = BTreeSet::new();
    let mut discovered = BTreeSet::new();
    for bytes in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
    {
        let path = std::str::from_utf8(bytes).context("input path is not UTF-8")?;
        relative(path)?;
        discovered.insert(path.to_owned());
        let workspace_manifest = path == "Cargo.toml" || path.ends_with("/Cargo.toml");
        let unselected_catalog = !matches!(scope, InputScope::Repository)
            && matches_root(path, CATALOG_DIRECTORY)
            && !roots.iter().any(|root| root == path);
        if !excluded(path)
            && !unselected_catalog
            && (matches!(scope, InputScope::Repository)
                || workspace_manifest
                || roots.iter().any(|selected| matches_root(path, selected)))
        {
            paths.insert(path.to_owned());
        }
    }
    for selected in roots {
        relative(selected)?;
        if !excluded(selected) && !root.join(selected).is_dir() {
            paths.insert(selected.to_owned());
        }
    }
    let mut files = Vec::new();
    let mut visited = BTreeSet::new();
    while let Some(path) = paths.pop_first() {
        if !visited.insert(path.clone()) {
            continue;
        }
        ensure!(visited.len() <= 100_000, "input manifest exceeds its bound");
        let absolute = root.join(&path);
        let metadata = match fs::symlink_metadata(&absolute) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // Tracked deletions and an equivalent fresh checkout have the
                // same bytes. Only explicit expected paths retain a missing marker.
                if roots.contains(&path) {
                    files.push(FileInput {
                        path,
                        content: FileContent::Missing,
                    });
                }
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        // Check parents too: an otherwise ordinary path may traverse a symlink.
        let resolved =
            fs::canonicalize(&absolute).context("input link is broken or inaccessible")?;
        let resolved = resolved
            .strip_prefix(root)
            .context("input link escapes repository")?;
        let resolved = resolved.to_str().context("input link is not UTF-8")?;
        relative(resolved)?;
        ensure!(
            resolved == path || discovered.contains(resolved),
            "input traverses a link to an ignored or undiscovered file"
        );
        ensure!(
            !excluded(resolved),
            "source cannot depend on its evidence output"
        );
        let content = if metadata.is_symlink() {
            ensure!(
                !absolute.is_dir(),
                "directory symlinks require an explicit qualified input boundary"
            );
            ensure!(
                discovered.contains(resolved),
                "input link targets an ignored or undiscovered file"
            );
            paths.insert(resolved.to_owned());
            FileContent::Symlink {
                target: fs::read_link(&absolute)?
                    .to_str()
                    .context("input link is not UTF-8")?
                    .to_owned(),
            }
        } else {
            ensure!(metadata.is_file(), "unsupported input entry: {path}");
            let mut file = fs::File::open(&absolute)?;
            let mut sha = Sha256::new();
            let mut count = 0;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                sha.update(&buffer[..read]);
                count += read as u64;
            }
            ensure!(
                count == metadata.len(),
                "input length changed while reading"
            );
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                // Checkout umasks change read/write permissions without changing
                // source. Preserve materialized execute bits for script inputs.
                0o644 | (metadata.permissions().mode() & 0o111)
            };
            #[cfg(not(unix))]
            let mode = 0o644;
            FileContent::File {
                sha256: format!("sha256:{}", hex::encode(sha.finalize())),
                bytes: count,
                mode,
            }
        };
        files.push(FileInput { path, content });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let hash = digest(&serde_json::to_vec(&(PLANNER_VERSION, scope, &files))?);
    Ok(SourceInputs {
        planner_version: PLANNER_VERSION,
        scope: scope.clone(),
        digest: hash,
        files,
    })
}
