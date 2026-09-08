use std::{ffi::OsString, fs, path::Path, process::Command};

use anyhow::{Context, Result, ensure};

pub(super) fn copy_fixture(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "configuration fixture cannot follow a symlink"
        );
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_fixture(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub(super) fn run_checked(
    program: &Path,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .envs(envs)
        .output()
        .with_context(|| format!("running {}", program.display()))?;
    ensure!(
        output.status.success(),
        "{} failed with {}\nstdout:\n{}\nstderr:\n{}",
        program.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).context("configuration command output is not UTF-8")
}

pub(super) fn contains(haystack: &str, needle: &str) -> Result<()> {
    ensure!(
        haystack.contains(needle),
        "expected output to contain `{needle}`\noutput:\n{haystack}"
    );
    Ok(())
}

pub(super) fn not_contains(haystack: &str, needle: &str) -> Result<()> {
    ensure!(
        !haystack.contains(needle),
        "expected output to omit `{needle}`\noutput:\n{haystack}"
    );
    Ok(())
}
