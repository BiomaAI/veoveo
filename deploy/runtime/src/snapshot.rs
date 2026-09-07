//! Exact Git input checks independent of worktree/index freshness hints.

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

use crate::{process::output_checked, sources::resolve_revision};

/// Checks only deployment inputs. Unrelated working files and build outputs do
/// not participate. The caller still owns the checkout through rendering.
pub(crate) struct SnapshotInputs {
    repository: PathBuf,
    revision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitFileMode {
    Regular,
    Executable,
}

struct GitFile {
    mode: GitFileMode,
    object: String,
}

impl SnapshotInputs {
    pub(crate) fn new(repository: &Path, revision: &str) -> Result<Self> {
        let repository = fs::canonicalize(repository).context("resolving source snapshot root")?;
        ensure!(
            resolve_revision(&repository, "HEAD")? == revision,
            "deployment input snapshot differs from its immutable revision"
        );
        Ok(Self {
            repository,
            revision: revision.to_owned(),
        })
    }

    pub(crate) fn file(&self, path: &Path) -> Result<()> {
        let relative = self.relative(path)?;
        let actual = BTreeMap::from([(
            relative.clone(),
            file_mode(&fs::symlink_metadata(self.repository.join(&relative))?)?,
        )]);
        self.verify(&relative, actual)
    }

    pub(crate) fn tree(&self, path: &Path) -> Result<()> {
        let relative = self.relative(path)?;
        ensure!(
            self.repository.join(&relative).is_dir(),
            "deployment input tree must be a directory"
        );
        let mut actual = BTreeMap::new();
        collect_files(&self.repository, &relative, &mut actual)?;
        self.verify(&relative, actual)
    }

    /// Resolves ordinary profile-relative `..` segments while rejecting symlinks
    /// at every visited step, including a symlink followed by `..`.
    fn relative(&self, path: &Path) -> Result<PathBuf> {
        let path = if path.is_absolute() {
            path.strip_prefix(&self.repository)
                .context("deployment input escapes its source repository")?
        } else {
            path
        };
        let mut relative = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => relative.push(part),
                Component::CurDir => continue,
                Component::ParentDir => {
                    ensure!(
                        relative.pop(),
                        "deployment input escapes its source repository"
                    );
                }
                _ => anyhow::bail!("deployment input must be repository-relative"),
            }
            let metadata = fs::symlink_metadata(self.repository.join(&relative))
                .context("reading deployment input path")?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "deployment input path cannot contain symlinks"
            );
        }
        ensure!(
            !relative.as_os_str().is_empty(),
            "deployment input cannot be the repository root"
        );
        ensure!(
            relative.to_str().is_some(),
            "deployment input path must be UTF-8"
        );
        Ok(relative)
    }

    fn verify(&self, relative: &Path, actual: BTreeMap<PathBuf, GitFileMode>) -> Result<()> {
        // Literal pathspecs prevent names such as [values].yaml from expanding.
        let pathspec = format!(
            ":(literal){}",
            relative.to_str().context("input path must be UTF-8")?
        );
        let tree = output_checked(
            "git",
            [
                "ls-tree",
                "-r",
                "-z",
                "--full-tree",
                &self.revision,
                "--",
                &pathspec,
            ],
            Some(&self.repository),
        )?;
        let mut expected = BTreeMap::new();
        for record in tree
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
        {
            let record = std::str::from_utf8(record).context("Git input tree must be UTF-8")?;
            let (header, name) = record.split_once('\t').context("invalid Git tree entry")?;
            let fields = header.split(' ').collect::<Vec<_>>();
            ensure!(
                fields.len() == 3 && fields[1] == "blob",
                "deployment input must contain only Git blobs"
            );
            let mode = match fields[0] {
                "100644" => GitFileMode::Regular,
                "100755" => GitFileMode::Executable,
                _ => anyhow::bail!("deployment input {name} has an unsupported Git file mode"),
            };
            expected.insert(
                PathBuf::from(name),
                GitFile {
                    mode,
                    object: fields[2].to_owned(),
                },
            );
        }
        ensure!(
            !expected.is_empty(),
            "deployment input {} is not tracked at its immutable revision",
            relative.display()
        );
        ensure!(
            actual.keys().eq(expected.keys()),
            "deployment input {} has added or missing files relative to its immutable revision",
            relative.display()
        );
        for (path, mode) in &actual {
            ensure!(
                *mode == expected[path].mode,
                "deployment input {} has a changed executable mode",
                path.display()
            );
        }
        // hash-object reads actual bytes without clean filters and without the
        // index's assume-unchanged/skip-worktree hints. Bound argv size by batch.
        let paths = actual.keys().collect::<Vec<_>>();
        for batch in paths.chunks(32) {
            let mut args = vec![
                "hash-object".to_owned(),
                "--no-filters".to_owned(),
                "--".to_owned(),
            ];
            for path in batch {
                args.push(
                    self.repository
                        .join(path)
                        .to_str()
                        .context("input path must be UTF-8")?
                        .to_owned(),
                );
            }
            let hashes = output_checked(
                "git",
                args.iter().map(String::as_str),
                Some(&self.repository),
            )?;
            let hashes = std::str::from_utf8(&hashes)
                .context("invalid Git object hashes")?
                .lines()
                .collect::<Vec<_>>();
            ensure!(
                hashes.len() == batch.len(),
                "Git omitted deployment input hashes"
            );
            for (path, hash) in batch.iter().zip(hashes) {
                ensure!(
                    hash == expected[*path].object,
                    "deployment input {} differs from its immutable revision",
                    path.display()
                );
            }
        }
        Ok(())
    }
}

fn collect_files(
    repository: &Path,
    relative: &Path,
    files: &mut BTreeMap<PathBuf, GitFileMode>,
) -> Result<()> {
    for entry in fs::read_dir(repository.join(relative)).context("reading deployment input tree")? {
        let entry = entry?;
        let path = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() {
            collect_files(repository, &path, files)?;
        } else {
            files.insert(path, file_mode(&metadata)?);
        }
    }
    Ok(())
}

fn file_mode(metadata: &fs::Metadata) -> Result<GitFileMode> {
    ensure!(
        metadata.is_file(),
        "deployment input cannot contain symlinks, directories, or special files"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(if metadata.permissions().mode() & 0o111 == 0 {
            GitFileMode::Regular
        } else {
            GitFileMode::Executable
        })
    }
    #[cfg(not(unix))]
    anyhow::bail!("deployment input verification requires Git executable modes")
}

#[cfg(test)]
mod tests;
