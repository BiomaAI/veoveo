use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Output, Stdio},
};

use anyhow::{Context, Result, bail};

pub(crate) fn output(
    program: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    directory: Option<&Path>,
) -> Result<Output> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let output = command
        .output()
        .with_context(|| format!("running {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output)
}

pub(crate) fn output_text(
    program: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    directory: Option<&Path>,
) -> Result<String> {
    let output = output(program, args, directory)?;
    String::from_utf8(output.stdout).context("command output is not UTF-8")
}

pub(crate) fn status(
    program: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    directory: Option<&Path>,
) -> Result<()> {
    status_with_env(program, args, &[], directory)
}

pub(crate) fn status_with_env(
    program: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    environment: &[(&str, &str)],
    directory: Option<&Path>,
) -> Result<()> {
    let mut command = Command::new(program);
    command
        .args(args)
        .envs(environment.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let status = command
        .status()
        .with_context(|| format!("running {program}"))?;
    if !status.success() {
        bail!("{program} failed with {status}");
    }
    Ok(())
}
