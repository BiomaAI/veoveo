use std::{fs, path::Path, process::Command};

use super::SnapshotInputs;

struct Repository {
    directory: tempfile::TempDir,
    revision: String,
}

impl Repository {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        git(root, &["init", "--quiet"]);
        git(root, &["config", "user.name", "Snapshot Test"]);
        git(root, &["config", "user.email", "snapshot@example.invalid"]);
        fs::create_dir(root.join("chart")).unwrap();
        fs::write(
            root.join("chart/Chart.yaml"),
            "apiVersion: v2\nname: fixture\nversion: 1.0.0\n",
        )
        .unwrap();
        fs::write(root.join("chart/values.yaml"), "replicas: 1\n").unwrap();
        fs::write(root.join(".gitignore"), "chart/ignored.yaml\noutput/\n").unwrap();
        git(root, &["add", "."]);
        git(
            root,
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
        );
        let revision = git(root, &["rev-parse", "HEAD"]);
        Self {
            directory,
            revision,
        }
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn snapshot(&self) -> SnapshotInputs {
        SnapshotInputs::new(self.root(), &self.revision).unwrap()
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn index_hints_cannot_hide_modified_chart_or_values_bytes() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let repo = Repository::new();
        let snapshot = repo.snapshot();
        snapshot.tree(Path::new("chart")).unwrap();
        git(repo.root(), &["update-index", flag, "chart/values.yaml"]);
        fs::write(repo.root().join("chart/values.yaml"), "replicas: 9\n").unwrap();
        // This is the old installation input check; Git's index hint masks drift.
        git(
            repo.root(),
            &["diff", "--quiet", "HEAD", "--", "chart/values.yaml"],
        );
        for result in [
            snapshot.file(Path::new("chart/values.yaml")),
            snapshot.tree(Path::new("chart")),
        ] {
            let error = result.unwrap_err().to_string();
            assert!(
                error.contains("differs from its immutable revision"),
                "{error}"
            );
            assert!(
                !error.contains("replicas: 9"),
                "file bytes must not enter diagnostics"
            );
        }
    }
}

#[test]
fn complete_chart_inventory_includes_ignored_files_but_excludes_unrelated_work() {
    let repo = Repository::new();
    let snapshot = repo.snapshot();
    fs::create_dir(repo.root().join("output")).unwrap();
    fs::write(repo.root().join("output/experiment.txt"), "unrelated").unwrap();
    fs::write(repo.root().join("notes.txt"), "unrelated").unwrap();
    snapshot.tree(Path::new("chart")).unwrap();
    fs::write(
        repo.root().join("chart/ignored.yaml"),
        "unexpected render input",
    )
    .unwrap();
    git(repo.root(), &["check-ignore", "chart/ignored.yaml"]);
    assert!(
        snapshot
            .tree(Path::new("chart"))
            .unwrap_err()
            .to_string()
            .contains("added or missing files")
    );
    fs::remove_file(repo.root().join("chart/ignored.yaml")).unwrap();
    snapshot.tree(Path::new("chart")).unwrap();
    fs::remove_file(repo.root().join("chart/values.yaml")).unwrap();
    assert!(
        snapshot
            .tree(Path::new("chart"))
            .unwrap_err()
            .to_string()
            .contains("added or missing files")
    );
}

#[test]
fn clean_filters_cannot_substitute_committed_bytes_for_rendered_bytes() {
    let repo = Repository::new();
    git(
        repo.root(),
        &[
            "config",
            "filter.normalized.clean",
            "sed 's/replicas: 9/replicas: 1/'",
        ],
    );
    fs::write(
        repo.root().join(".gitattributes"),
        "chart/values.yaml filter=normalized\n",
    )
    .unwrap();
    fs::write(repo.root().join("chart/values.yaml"), "replicas: 9\n").unwrap();
    // The clean filter makes the worktree appear unchanged, including a direct
    // filtered Git hash. The deployment reads the literal worktree bytes.
    git(
        repo.root(),
        &["diff", "--quiet", "HEAD", "--", "chart/values.yaml"],
    );
    assert!(
        repo.snapshot()
            .file(Path::new("chart/values.yaml"))
            .unwrap_err()
            .to_string()
            .contains("differs from its immutable revision")
    );
}

#[test]
fn literal_paths_and_parent_segments_are_verified_without_pathspec_expansion() {
    let mut repo = Repository::new();
    for name in ["[values].yaml", "v.yaml", "line\nbreak.yaml"] {
        fs::write(repo.root().join("chart").join(name), "replicas: 1\n").unwrap();
    }
    git(repo.root(), &["add", "chart"]);
    git(
        repo.root(),
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "literal names",
        ],
    );
    repo.revision = git(repo.root(), &["rev-parse", "HEAD"]);
    let snapshot = repo.snapshot();
    snapshot.tree(Path::new("chart")).unwrap();
    snapshot.file(Path::new("chart/[values].yaml")).unwrap();
    snapshot
        .file(Path::new("chart/../chart/line\nbreak.yaml"))
        .unwrap();
    fs::write(
        repo.root().join("chart/v.yaml"),
        "unrelated selected-file change",
    )
    .unwrap();
    snapshot.file(Path::new("chart/[values].yaml")).unwrap();
    assert!(snapshot.tree(Path::new("chart")).is_err());
    assert!(
        snapshot
            .file(Path::new("../outside.yaml"))
            .unwrap_err()
            .to_string()
            .contains("escapes")
    );
}

#[test]
fn untracked_staged_and_wrong_revision_inputs_are_rejected() {
    let repo = Repository::new();
    fs::write(repo.root().join("uncommitted.yaml"), "replicas: 1\n").unwrap();
    for staged in [false, true] {
        if staged {
            git(repo.root(), &["add", "uncommitted.yaml"]);
        }
        assert!(
            repo.snapshot()
                .file(Path::new("uncommitted.yaml"))
                .unwrap_err()
                .to_string()
                .contains("not tracked at its immutable revision")
        );
    }
    assert!(SnapshotInputs::new(repo.root(), &"a".repeat(40)).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_and_executable_mode_drift_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let repo = Repository::new();
    let snapshot = repo.snapshot();
    fs::set_permissions(
        repo.root().join("chart/values.yaml"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(
        snapshot
            .file(Path::new("chart/values.yaml"))
            .unwrap_err()
            .to_string()
            .contains("changed executable mode")
    );
    fs::set_permissions(
        repo.root().join("chart/values.yaml"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    symlink("chart", repo.root().join("alias")).unwrap();
    for path in ["alias/values.yaml", "alias/../chart/values.yaml"] {
        assert!(
            snapshot
                .file(Path::new(path))
                .unwrap_err()
                .to_string()
                .contains("symlinks")
        );
    }
    symlink("values.yaml", repo.root().join("chart/link.yaml")).unwrap();
    assert!(
        snapshot
            .tree(Path::new("chart"))
            .unwrap_err()
            .to_string()
            .contains("symlinks")
    );
}
