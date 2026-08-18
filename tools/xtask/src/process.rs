use std::{
    env,
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

pub(crate) fn cargo_status(
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    directory: Option<&Path>,
) -> Result<()> {
    cargo_status_with_env(args, &[], directory)
}

pub(crate) fn cargo_status_with_env(
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    environment: &[(&str, &str)],
    directory: Option<&Path>,
) -> Result<()> {
    let mut command = Command::new("cargo");
    command
        .args(args)
        .envs(environment.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    remove_parent_cargo_package_environment(&mut command);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let status = command.status().context("running nested Cargo")?;
    if !status.success() {
        bail!("nested Cargo failed with {status}");
    }
    Ok(())
}

pub(crate) fn remove_parent_cargo_package_environment(command: &mut Command) {
    for (key, _) in env::vars_os() {
        if is_cargo_package_environment(&key) {
            command.env_remove(key);
        }
    }
}

fn is_cargo_package_environment(key: &OsStr) -> bool {
    let key = key.to_string_lossy();
    matches!(
        key.as_ref(),
        "CARGO"
            | "CARGO_BIN_NAME"
            | "CARGO_CRATE_NAME"
            | "CARGO_ENCODED_RUSTFLAGS"
            | "CARGO_MAKEFLAGS"
            | "CARGO_MANIFEST_DIR"
            | "CARGO_MANIFEST_PATH"
            | "CARGO_PRIMARY_PACKAGE"
            | "CARGO_TARGET_TMPDIR"
            | "DEBUG"
            | "HOST"
            | "NUM_JOBS"
            | "OPT_LEVEL"
            | "OUT_DIR"
            | "PROFILE"
            | "RUSTC"
            | "RUSTDOC"
            | "TARGET"
    ) || key.starts_with("CARGO_CFG_")
        || key.starts_with("CARGO_FEATURE_")
        || key.starts_with("CARGO_PKG_")
        || key.starts_with("DEP_")
}

#[cfg(test)]
mod tests {
    use super::is_cargo_package_environment;
    use std::ffi::OsStr;

    #[test]
    fn nested_cargo_drops_parent_package_scope_but_keeps_user_configuration() {
        for key in [
            "CARGO_MANIFEST_DIR",
            "CARGO_PKG_NAME",
            "CARGO_FEATURE_RUNTIME",
            "CARGO_CFG_TARGET_ARCH",
            "DEP_NATIVE_ROOT",
            "OUT_DIR",
        ] {
            assert!(is_cargo_package_environment(OsStr::new(key)), "{key}");
        }
        for key in [
            "CARGO_HOME",
            "CARGO_TARGET_DIR",
            "CARGO_NET_OFFLINE",
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
        ] {
            assert!(!is_cargo_package_environment(OsStr::new(key)), "{key}");
        }
    }
}
