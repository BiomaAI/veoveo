use std::{ffi::OsString, path::Path, process::Command};

use anyhow::{Context, Result, ensure};

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
