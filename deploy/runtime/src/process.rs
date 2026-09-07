use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub(crate) fn kubectl_apply_value(context: &str, value: &Value) -> Result<()> {
    let mut child = Command::new("kubectl")
        .args(["--context", context, "apply", "-f", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .context("spawning kubectl apply")?;
    serde_json::to_writer(
        child
            .stdin
            .as_mut()
            .context("kubectl stdin is unavailable")?,
        value,
    )?;
    child
        .stdin
        .take()
        .context("kubectl stdin is unavailable")?
        .flush()?;
    let status = child.wait().context("waiting for kubectl apply")?;
    ensure!(status.success(), "kubectl apply failed with {status}");
    Ok(())
}

pub(crate) fn output_checked<'a>(
    program: &str,
    args: impl IntoIterator<Item = &'a str>,
    directory: Option<&Path>,
) -> Result<Vec<u8>> {
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
    Ok(output.stdout)
}

pub(crate) fn status_checked<'a>(
    program: &str,
    args: impl IntoIterator<Item = &'a str>,
    environment: &[(&str, &str)],
    directory: Option<&Path>,
) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args).envs(environment.iter().copied());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let status = command
        .status()
        .with_context(|| format!("running {program}"))?;
    ensure!(status.success(), "{program} failed with {status}");
    Ok(())
}

pub(crate) fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}
