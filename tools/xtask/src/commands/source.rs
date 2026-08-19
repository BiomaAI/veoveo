use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{context::RepositoryContext, process};

// Publication materializes an exact revision, not every LFS payload in that
// repository. Selected LFS build inputs remain the responsibility of the
// build phase that consumes them.
const GIT_SKIP_LFS_SMUDGE: &[(&str, &str)] = &[("GIT_LFS_SKIP_SMUDGE", "1")];

#[derive(Debug)]
pub(crate) struct PublicationSource {
    path: PathBuf,
    revision: String,
    _lock: File,
}

impl PublicationSource {
    pub(crate) fn prepare(repository: &RepositoryContext, revision: &str) -> Result<Self> {
        let revision = resolve_revision(repository.root(), revision)?;
        let layout = PublicationLayout::discover(repository)?;
        fs::create_dir_all(&layout.directory).with_context(|| {
            format!(
                "creating publication source directory {}",
                layout.directory.display()
            )
        })?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&layout.lock)
            .with_context(|| format!("opening publication lock {}", layout.lock.display()))?;
        File::lock(&lock)
            .with_context(|| format!("locking publication source identity {}", layout.source_id))?;

        let registered = registered_worktrees(repository.root())?;
        let source_exists = layout.source.exists();
        let source_registered = registered.iter().any(|path| path == &layout.source);
        match (source_exists, source_registered) {
            (false, false) => {
                process::status_with_env(
                    "git",
                    [
                        "worktree",
                        "add",
                        "--detach",
                        path_text(&layout.source)?,
                        revision.as_str(),
                    ],
                    GIT_SKIP_LFS_SMUDGE,
                    Some(repository.root()),
                )?;
            }
            (true, true) => {
                require_clean(&layout.source)?;
                process::status_with_env(
                    "git",
                    ["checkout", "--detach", revision.as_str()],
                    GIT_SKIP_LFS_SMUDGE,
                    Some(&layout.source),
                )?;
            }
            (true, false) => {
                bail!(
                    "publication source {} exists but is not a registered Git worktree; inspect it manually",
                    layout.source.display()
                );
            }
            (false, true) => {
                bail!(
                    "publication source {} is registered but missing; repair the Git worktree manually",
                    layout.source.display()
                );
            }
        }

        require_clean(&layout.source)?;
        let head = process::output_text("git", ["rev-parse", "HEAD"], Some(&layout.source))?;
        ensure!(
            head.trim() == revision,
            "publication worktree resolved to {} instead of {revision}",
            head.trim()
        );

        Ok(Self {
            path: layout.source,
            revision,
            _lock: lock,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }
}

#[derive(Debug)]
struct PublicationLayout {
    source_id: String,
    directory: PathBuf,
    source: PathBuf,
    lock: PathBuf,
}

impl PublicationLayout {
    fn discover(repository: &RepositoryContext) -> Result<Self> {
        let common = process::output_text(
            "git",
            ["rev-parse", "--path-format=absolute", "--git-common-dir"],
            Some(repository.root()),
        )?;
        let common = fs::canonicalize(common.trim())
            .with_context(|| format!("resolving Git common directory {}", common.trim()))?;
        let main_worktree = common
            .parent()
            .context("Git common directory has no parent worktree")?;
        let origin = process::output_text(
            "git",
            ["config", "--get", "remote.origin.url"],
            Some(repository.root()),
        )
        .context("publication requires remote.origin.url")?;
        let object_format = process::output_text(
            "git",
            ["rev-parse", "--show-object-format"],
            Some(repository.root()),
        )?;
        let identity = format!(
            "{}\n{}\n{}",
            normalize_origin(origin.trim())?,
            common.display(),
            object_format.trim()
        );
        let source_id = hex::encode(Sha256::digest(identity.as_bytes()));
        let target = main_worktree.join("target");
        let target = match fs::symlink_metadata(&target) {
            Ok(_) => fs::canonicalize(&target).with_context(|| {
                format!("resolving Cargo target directory {}", target.display())
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => target,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspecting Cargo target directory {}", target.display())
                });
            }
        };
        let directory = target.join("veoveo-xtask/publication").join(&source_id);
        Ok(Self {
            source_id,
            source: directory.join("source"),
            lock: directory.join("lock"),
            directory,
        })
    }
}

pub(crate) fn source_hash(repository: &RepositoryContext) -> Result<String> {
    Ok(PublicationLayout::discover(repository)?.source_id)
}

pub(crate) fn resolve_revision(repository: &Path, revision: &str) -> Result<String> {
    ensure!(!revision.trim().is_empty(), "revision cannot be empty");
    let expression = format!("{revision}^{{commit}}");
    let resolved = process::output_text(
        "git",
        ["rev-parse", "--verify", expression.as_str()],
        Some(repository),
    )
    .with_context(|| format!("resolving publication revision {revision}"))?;
    let resolved = resolved.trim();
    ensure!(
        matches!(resolved.len(), 40 | 64) && resolved.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git resolved an invalid object ID: {resolved}"
    );
    Ok(resolved.to_ascii_lowercase())
}

fn registered_worktrees(repository: &Path) -> Result<Vec<PathBuf>> {
    let output =
        process::output_text("git", ["worktree", "list", "--porcelain"], Some(repository))?;
    output
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(|path| {
            let path = PathBuf::from(path);
            if path.exists() {
                fs::canonicalize(&path).with_context(|| {
                    format!("resolving registered Git worktree {}", path.display())
                })
            } else {
                Ok(path)
            }
        })
        .collect()
}

fn require_clean(source: &Path) -> Result<()> {
    let status = process::output_text(
        "git",
        ["status", "--porcelain=v1", "--untracked-files=all"],
        Some(source),
    )?;
    ensure!(
        status.trim().is_empty(),
        "publication source {} is dirty; inspect it manually\n{}",
        source.display(),
        status.trim_end()
    );
    Ok(())
}

pub(crate) fn normalize_origin(origin: &str) -> Result<String> {
    let origin = origin.trim().trim_end_matches('/');
    ensure!(!origin.is_empty(), "remote.origin.url cannot be empty");
    let expanded = if !origin.contains("://") {
        if let Some((authority, path)) = origin.split_once(':') {
            if authority.contains('@') && !path.starts_with('/') {
                format!("ssh://{authority}/{}", path.trim_start_matches('/'))
            } else {
                origin.to_owned()
            }
        } else {
            origin.to_owned()
        }
    } else {
        origin.to_owned()
    };
    if let Ok(mut url) = Url::parse(&expanded) {
        url.set_query(None);
        url.set_fragment(None);
        let normalized_path = url
            .path()
            .trim_end_matches('/')
            .strip_suffix(".git")
            .unwrap_or_else(|| url.path().trim_end_matches('/'))
            .to_owned();
        url.set_path(&normalized_path);
        return Ok(url.to_string().trim_end_matches('/').to_owned());
    }
    let path =
        fs::canonicalize(&expanded).with_context(|| format!("normalizing Git origin {origin}"))?;
    Ok(format!("file://{}", path.display()))
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File, FileTimes},
        path::Path,
        process::Command,
        time::{Duration, SystemTime},
    };

    use tempfile::TempDir;

    use super::{PublicationSource, normalize_origin};
    use crate::context::RepositoryContext;

    #[test]
    fn normalizes_scp_origins() {
        assert_eq!(
            normalize_origin("git@github.com:BiomaAI/veoveo.git").expect("normalize origin"),
            "ssh://git@github.com/BiomaAI/veoveo"
        );
    }

    #[test]
    fn preserves_unchanged_path_mtime_between_revisions() {
        let temporary = TempDir::new().expect("temporary repository");
        let repository = temporary.path().join("repository");
        fs::create_dir(&repository).expect("create repository");
        git(&repository, ["init"]);
        git(&repository, ["config", "user.email", "test@example.com"]);
        git(&repository, ["config", "user.name", "Veoveo Test"]);
        git(&repository, ["config", "commit.gpgsign", "false"]);
        git(
            &repository,
            [
                "remote",
                "add",
                "origin",
                "git@github.com:BiomaAI/fixture.git",
            ],
        );
        fs::write(repository.join("unchanged.txt"), "stable\n").expect("write stable file");
        fs::write(repository.join("changed.txt"), "first\n").expect("write changed file");
        git(&repository, ["add", "."]);
        git(&repository, ["commit", "-m", "first"]);
        let first = git_output(&repository, ["rev-parse", "HEAD"]);

        fs::write(repository.join("changed.txt"), "second\n").expect("update changed file");
        git(&repository, ["commit", "-am", "second"]);
        let second = git_output(&repository, ["rev-parse", "HEAD"]);

        let context = RepositoryContext::discover(&repository).expect("discover repository");
        let source = PublicationSource::prepare(&context, first.trim()).expect("first source");
        let stable = source.path().join("unchanged.txt");
        let changed = source.path().join("changed.txt");
        let marker = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        File::options()
            .write(true)
            .open(&stable)
            .expect("open stable file")
            .set_times(FileTimes::new().set_modified(marker))
            .expect("set stable mtime");
        File::options()
            .write(true)
            .open(&changed)
            .expect("open changed file")
            .set_times(FileTimes::new().set_modified(marker))
            .expect("set changed mtime");
        drop(source);

        let source = PublicationSource::prepare(&context, second.trim()).expect("second source");
        assert_eq!(
            fs::metadata(source.path().join("unchanged.txt"))
                .expect("stable metadata")
                .modified()
                .expect("stable mtime"),
            marker
        );
        assert_ne!(
            fs::metadata(source.path().join("changed.txt"))
                .expect("changed metadata")
                .modified()
                .expect("changed mtime"),
            marker
        );
        assert_eq!(source.revision(), second.trim());
    }

    #[cfg(unix)]
    #[test]
    fn reuses_publication_worktree_through_symlinked_target() {
        let temporary = TempDir::new().expect("temporary repository");
        let repository = temporary.path().join("repository");
        let external_target = temporary.path().join("external-target");
        fs::create_dir(&repository).expect("create repository");
        fs::create_dir(&external_target).expect("create external target");
        std::os::unix::fs::symlink(&external_target, repository.join("target"))
            .expect("link external target");
        git(&repository, ["init"]);
        git(&repository, ["config", "user.email", "test@example.com"]);
        git(&repository, ["config", "user.name", "Veoveo Test"]);
        git(&repository, ["config", "commit.gpgsign", "false"]);
        git(
            &repository,
            [
                "remote",
                "add",
                "origin",
                "git@github.com:BiomaAI/symlink-fixture.git",
            ],
        );
        fs::write(repository.join("tracked.txt"), "first\n").expect("write tracked file");
        git(&repository, ["add", "."]);
        git(&repository, ["commit", "-m", "first"]);
        let first = git_output(&repository, ["rev-parse", "HEAD"]);

        fs::write(repository.join("tracked.txt"), "second\n").expect("update tracked file");
        git(&repository, ["commit", "-am", "second"]);
        let second = git_output(&repository, ["rev-parse", "HEAD"]);
        let context = RepositoryContext::discover(&repository).expect("discover repository");

        let source = PublicationSource::prepare(&context, first.trim()).expect("first source");
        assert!(source.path().starts_with(&external_target));
        drop(source);

        let source = PublicationSource::prepare(&context, second.trim()).expect("reuse source");
        assert_eq!(source.revision(), second.trim());
        assert_eq!(
            fs::read_to_string(source.path().join("tracked.txt")).expect("read tracked file"),
            "second\n"
        );
    }

    fn git<const N: usize>(repository: &Path, args: [&str; N]) {
        let status = Command::new("git")
            .current_dir(repository)
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success());
    }

    fn git_output<const N: usize>(repository: &Path, args: [&str; N]) -> String {
        let output = Command::new("git")
            .current_dir(repository)
            .args(args)
            .output()
            .expect("run git");
        assert!(output.status.success());
        String::from_utf8(output.stdout).expect("Git output")
    }
}
