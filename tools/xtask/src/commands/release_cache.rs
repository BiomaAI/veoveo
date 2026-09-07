//! Reclaim regenerable host Cargo outputs without evicting compiler libraries.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use crate::{ReleaseCacheArgs, context::RepositoryContext};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Kind {
    ExecutableCopy,
    IncrementalVariant,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    path: PathBuf,
    kind: Kind,
    modified_at_unix_seconds: u64,
    #[serde(skip)]
    modified: SystemTime,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CachePlan {
    schema_version: &'static str,
    root: PathBuf,
    minimum_age_days: u64,
    candidates: Vec<Candidate>,
    reclaimable_bytes: u64,
    applied: bool,
}

pub(crate) fn run(repository: &RepositoryContext, args: &ReleaseCacheArgs) -> Result<()> {
    ensure!(
        args.older_than_days >= 1,
        "cache retention must be at least one day"
    );
    let root = fs::canonicalize(repository.root().join("target/debug"))
        .context("locating this worktree's Cargo debug output")?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(".cargo-lock"))?;
    // Cargo uses this same advisory lock for mutations of the profile directory.
    File::lock(&lock).context("waiting for Cargo build-directory lock before cache maintenance")?;
    let mut plan = plan(&root, args.older_than_days, SystemTime::now())?;
    println!(
        "Cargo cache: {} older executable copies, {} older incremental variants, {:.2} GiB reclaimable",
        plan.candidates
            .iter()
            .filter(|candidate| matches!(candidate.kind, Kind::ExecutableCopy))
            .count(),
        plan.candidates
            .iter()
            .filter(|candidate| matches!(candidate.kind, Kind::IncrementalVariant))
            .count(),
        plan.reclaimable_bytes as f64 / 1024_f64.powi(3)
    );
    let output = if args.output.is_absolute() {
        args.output.clone()
    } else {
        repository.root().join(&args.output)
    };
    ensure!(
        !output.starts_with(&root),
        "cache maintenance evidence must be outside the cache being maintained"
    );
    write_plan(&output, &plan)?;
    if args.apply {
        for candidate in &plan.candidates {
            let path = root.join(&candidate.path);
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                metadata.modified()? == candidate.modified,
                "cache candidate changed after inspection: {}",
                path.display()
            );
            match candidate.kind {
                Kind::ExecutableCopy => {
                    ensure!(
                        executable_copy(&path, &metadata)?,
                        "cache candidate is no longer an unlinked executable copy: {}",
                        path.display()
                    );
                    fs::remove_file(&path).with_context(|| {
                        format!("removing Cargo executable copy {}", path.display())
                    })?;
                }
                Kind::IncrementalVariant => {
                    ensure!(
                        metadata.is_dir() && !metadata.is_symlink(),
                        "incremental candidate is no longer a directory: {}",
                        path.display()
                    );
                    fs::remove_dir_all(&path).with_context(|| {
                        format!("removing Cargo incremental variant {}", path.display())
                    })?;
                }
            }
        }
        plan.applied = true;
        write_plan(&output, &plan)?;
        println!(
            "Removed the planned Cargo outputs; dependency libraries and current executable links were retained"
        );
    } else {
        println!("Inspection only; --apply removes this class of regenerable output");
    }
    println!("Cache maintenance evidence: {}", output.display());
    Ok(())
}

fn write_plan(path: &Path, plan: &CachePlan) -> Result<()> {
    let parent = path.parent().context("cache evidence has no parent")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, plan)?;
    file.write_all(b"\n")?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn plan(root: &Path, age_days: u64, now: SystemTime) -> Result<CachePlan> {
    for name in ["deps", "incremental"] {
        let path = root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.is_symlink(),
                "Cargo cache directory must not redirect outside the selected profile: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let age = Duration::from_secs(
        age_days
            .checked_mul(86400)
            .context("cache age is too large")?,
    );
    let cutoff = now
        .checked_sub(age)
        .context("cache retention predates the Unix epoch")?;
    let mut candidates = Vec::new();
    let active = active_executables();
    for path in entries(&root.join("deps"))? {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.modified()? < cutoff
            && !active.contains(&path)
            && executable_copy(&path, &metadata)?
        {
            candidates.push(candidate(
                root,
                path,
                Kind::ExecutableCopy,
                metadata.modified()?,
            )?);
        }
    }
    let mut variants = BTreeMap::<String, Vec<(SystemTime, PathBuf)>>::new();
    for path in entries(&root.join("incremental"))? {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || metadata.is_symlink() {
            continue;
        }
        let Some((name, suffix)) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.rsplit_once('-'))
        else {
            continue;
        };
        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            continue;
        }
        variants
            .entry(name.to_owned())
            .or_default()
            .push((metadata.modified()?, path));
    }
    for variants in variants.values_mut() {
        variants.sort();
        variants.pop(); // Always retain the newest variant for this crate.
        for (modified, path) in variants.iter().filter(|(modified, _)| *modified < cutoff) {
            candidates.push(candidate(
                root,
                path.clone(),
                Kind::IncrementalVariant,
                *modified,
            )?);
        }
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    let reclaimable_bytes = reclaimable_bytes(root, &candidates)?;
    Ok(CachePlan {
        schema_version: "veoveo.io/cargo-cache-maintenance/v1",
        root: root.to_owned(),
        minimum_age_days: age_days,
        candidates,
        reclaimable_bytes,
        applied: false,
    })
}

fn candidate(root: &Path, path: PathBuf, kind: Kind, modified: SystemTime) -> Result<Candidate> {
    let path = path
        .strip_prefix(root)
        .context("cache candidate escaped its root")?
        .to_owned();
    ensure!(
        path.components().count() == 2,
        "unexpected Cargo cache candidate {}",
        path.display()
    );
    Ok(Candidate {
        path,
        kind,
        modified_at_unix_seconds: modified.duration_since(UNIX_EPOCH)?.as_secs(),
        modified,
    })
}

fn entries(path: &Path) -> Result<Vec<PathBuf>> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .map(|entry| entry.map(|entry| entry.path()).map_err(Into::into))
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn active_executables() -> BTreeSet<PathBuf> {
    entries(Path::new("/proc"))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| fs::read_link(path.join("exe")).ok())
        .collect()
}

#[cfg(unix)]
fn executable_copy(path: &Path, metadata: &fs::Metadata) -> Result<bool> {
    use std::{io::Read, os::unix::fs::MetadataExt};
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.mode() & 0o111 == 0 {
        return Ok(false);
    }
    let Some((_, hash)) = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.rsplit_once('-'))
    else {
        return Ok(false);
    };
    if hash.len() != 16 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(false);
    }
    let mut magic = [0; 4];
    Ok(File::open(path)?.read_exact(&mut magic).is_ok() && magic == *b"\x7fELF")
}

#[cfg(not(unix))]
fn executable_copy(_: &Path, _: &fs::Metadata) -> Result<bool> {
    anyhow::bail!("Cargo cache maintenance requires a Unix build host")
}

#[cfg(unix)]
fn reclaimable_bytes(root: &Path, candidates: &[Candidate]) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;
    let mut inodes = BTreeMap::<(u64, u64), (u64, u64, u64)>::new();
    let mut pending = candidates
        .iter()
        .map(|candidate| root.join(&candidate.path))
        .collect::<Vec<_>>();
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            pending.extend(entries(&path)?);
        } else {
            let entry = inodes.entry((metadata.dev(), metadata.ino())).or_insert((
                0,
                metadata.nlink(),
                metadata.blocks() * 512,
            ));
            entry.0 += 1;
        }
    }
    // A link retained elsewhere prevents those blocks from being reclaimed.
    Ok(inodes
        .values()
        .filter(|(selected, total, _)| selected == total)
        .map(|(_, _, bytes)| bytes)
        .sum())
}

#[cfg(not(unix))]
fn reclaimable_bytes(_: &Path, _: &[Candidate]) -> Result<u64> {
    anyhow::bail!("Cargo cache maintenance requires a Unix build host")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn retains_libraries_current_links_recent_files_and_latest_incremental_variant() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let now = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
        let old = now - Duration::from_secs(20 * 86400);
        fs::create_dir_all(root.join("deps")).unwrap();
        fs::create_dir_all(root.join("incremental")).unwrap();
        for (name, modified) in [
            ("test-aaaaaaaaaaaaaaaa", old),
            ("current-bbbbbbbbbbbbbbbb", old),
            ("recent-cccccccccccccccc", now),
            ("libtest-dddddddddddddddd.rlib", old),
        ] {
            let path = root.join("deps").join(name);
            fs::write(&path, b"\x7fELFfixture").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            File::open(&path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(modified))
                .unwrap();
        }
        fs::hard_link(
            root.join("deps/current-bbbbbbbbbbbbbbbb"),
            root.join("current"),
        )
        .unwrap();
        for (name, modified) in [
            ("crate-old", old),
            ("crate-latest", old + Duration::from_secs(1)),
            ("another-only", old),
        ] {
            let path = root.join("incremental").join(name);
            fs::create_dir(&path).unwrap();
            File::open(path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(modified))
                .unwrap();
        }
        let plan = plan(root, 7, now).unwrap();
        assert_eq!(
            plan.candidates
                .iter()
                .map(|candidate| candidate.path.as_path())
                .collect::<Vec<_>>(),
            [
                Path::new("deps/test-aaaaaaaaaaaaaaaa"),
                Path::new("incremental/crate-old")
            ]
        );
    }

    #[test]
    fn refuses_redirected_cache_directories() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(other.path(), root.path().join("deps")).unwrap();
        assert!(
            plan(root.path(), 7, SystemTime::now())
                .err()
                .unwrap()
                .to_string()
                .contains("must not redirect")
        );
    }

    #[test]
    fn maintenance_lock_blocks_cargo_builds() {
        use std::{
            io::{BufRead, BufReader},
            process::{Command, Stdio},
            sync::mpsc,
        };
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("src")).unwrap();
        fs::write(project.path().join("Cargo.toml"), "[package]\nname = \"cache-lock-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").unwrap();
        fs::write(project.path().join("src/lib.rs"), "pub fn fixture() {}\n").unwrap();
        let target = project.path().join("target");
        fs::create_dir_all(target.join("debug")).unwrap();
        let lock = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(target.join("debug/.cargo-lock"))
            .unwrap();
        File::lock(&lock).unwrap();
        let mut cargo = Command::new("cargo")
            .args(["check", "--offline", "--target-dir"])
            .arg(&target)
            .current_dir(project.path())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = cargo.stderr.take().unwrap();
        let (send, receive) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = send.send(line);
            }
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut observed = Vec::new();
        let mut blocked = false;
        while let Ok(line) =
            receive.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        {
            blocked = line.contains("Blocking waiting for file lock on artifact directory");
            observed.push(line);
            if blocked {
                break;
            }
        }
        drop(lock);
        let status = cargo.wait().unwrap();
        reader.join().unwrap();
        assert!(
            blocked,
            "Cargo must honor the same build-directory lock before cache deletion: {observed:?}"
        );
        assert!(status.success());
    }
}
