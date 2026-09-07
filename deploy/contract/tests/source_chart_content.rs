use std::{fs, path::Path, process::Command};

use tempfile::TempDir;
use veoveo_deploy_contract::source_chart_content_digest;

fn chart() -> TempDir {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("chart/templates")).unwrap();
    fs::write(
        root.path().join("chart/Chart.yaml"),
        "apiVersion: v2\nname: fixture\nversion: 1.0.0\n",
    )
    .unwrap();
    fs::write(root.path().join("chart/values.yaml"), "replicas: 1\n").unwrap();
    fs::write(
        root.path().join("chart/templates/config.yaml"),
        "kind: ConfigMap\n",
    )
    .unwrap();
    root
}

fn digest(root: &Path) -> String {
    source_chart_content_digest(root, Path::new("chart"))
        .unwrap()
        .to_string()
}

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {:?}", output.stderr);
    output.stdout
}

fn commit(root: &Path, message: &str) {
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    );
}

#[test]
fn unrelated_commits_preserve_chart_content_while_commit_archives_change() {
    let root = chart();
    git(root.path(), &["init", "--quiet"]);
    git(root.path(), &["config", "user.name", "Veoveo chart test"]);
    git(
        root.path(),
        &["config", "user.email", "chart@example.invalid"],
    );
    commit(root.path(), "chart");
    let before = digest(root.path());
    let archive_before = git(root.path(), &["archive", "--format=tar", "HEAD", "chart"]);
    let tree_before = git(root.path(), &["rev-parse", "HEAD:chart"]);
    fs::write(root.path().join("README.md"), "Unrelated documentation\n").unwrap();
    commit(root.path(), "documentation");
    assert_eq!(tree_before, git(root.path(), &["rev-parse", "HEAD:chart"]));
    assert_ne!(
        archive_before,
        git(root.path(), &["archive", "--format=tar", "HEAD", "chart"])
    );
    assert_eq!(before, digest(root.path()));

    // Another checkout has independent filesystem metadata but identical contents.
    let checkout = TempDir::new().unwrap();
    git(
        checkout.path(),
        &["clone", "--quiet", root.path().to_str().unwrap(), "source"],
    );
    assert_eq!(before, digest(&checkout.path().join("source")));
}

#[test]
fn chart_files_cannot_escape_the_digest_through_git_export_attributes() {
    let root = chart();
    fs::write(
        root.path().join("chart/.gitattributes"),
        "values.yaml export-ignore\n",
    )
    .unwrap();
    let before = digest(root.path());
    fs::write(root.path().join("chart/values.yaml"), "replicas: 2\n").unwrap();
    assert_ne!(before, digest(root.path()));
}

#[test]
fn paths_bytes_and_executable_modes_participate_in_chart_identity() {
    let root = chart();
    let before = digest(root.path());
    assert_eq!(
        before,
        "sha256:20a14828509bd3a7a0e20f4b86bb92860ff327032fe116ebe46c555bae6de119"
    );
    let config = root.path().join("chart/templates/config.yaml");
    fs::rename(&config, root.path().join("chart/templates/renamed.yaml")).unwrap();
    assert_ne!(before, digest(root.path()));
    fs::rename(root.path().join("chart/templates/renamed.yaml"), &config).unwrap();
    assert_eq!(before, digest(root.path()));
    fs::write(&config, "kind: ConfigMap").unwrap();
    assert_ne!(before, digest(root.path()));
    fs::write(&config, "kind: ConfigMap\n").unwrap();
    assert_eq!(before, digest(root.path()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config, fs::Permissions::from_mode(0o755)).unwrap();
        assert_ne!(before, digest(root.path()));
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            before,
            digest(root.path()),
            "read/write permissions are filesystem metadata"
        );
    }
}

#[test]
fn chart_paths_and_symlinks_cannot_extend_the_input_boundary() {
    let root = chart();
    for path in [
        "",
        ".",
        "chart/..",
        "chart/../chart",
        "chart//templates",
        "/chart",
    ] {
        assert!(
            source_chart_content_digest(root.path(), Path::new(path)).is_err(),
            "{path}"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        fs::write(root.path().join("outside.yaml"), "external: value\n").unwrap();
        symlink(
            root.path().join("outside.yaml"),
            root.path().join("chart/external.yaml"),
        )
        .unwrap();
        assert!(source_chart_content_digest(root.path(), Path::new("chart")).is_err());
        fs::remove_file(root.path().join("chart/external.yaml")).unwrap();
        symlink(root.path().join("chart"), root.path().join("alias")).unwrap();
        assert!(source_chart_content_digest(root.path(), Path::new("alias")).is_err());
    }
}
