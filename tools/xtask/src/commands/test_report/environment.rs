//! Public tool versions and explicit refusal of unmodeled execution overrides.
use std::{collections::BTreeMap, env, path::Path, process::Command};

use anyhow::{Context, Result, ensure};

use super::model::{
    CheckDefinition, EnvironmentIdentity, EvidenceClass, RuntimeIdentity, ToolIdentity, Toolchain,
};

fn version(root: &Path, program: &str, arguments: &[&str]) -> Result<ToolIdentity> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .env_remove("LD_LIBRARY_PATH")
        .output()
        .context("observing declared tool version")?;
    ensure!(
        output.status.success() && output.stdout.len() <= 4096,
        "declared tool version unavailable"
    );
    let value = String::from_utf8(output.stdout).context("tool version is not UTF-8")?;
    ensure!(
        !value.trim().is_empty(),
        "declared tool returned no version"
    );
    Ok(ToolIdentity {
        name: program.to_owned(),
        version: value.trim().to_owned(),
    })
}

pub(super) fn observe(
    root: &Path,
    definition: Option<&CheckDefinition>,
) -> Result<EnvironmentIdentity> {
    let mut toolchains = Vec::new();
    if let Some(definition) = definition {
        for tool in &definition.toolchains {
            match tool {
                Toolchain::Rust => {
                    toolchains.push(version(root, "rustc", &["-vV"])?);
                    toolchains.push(version(root, "cargo", &["-V"])?);
                    toolchains.push(version(root, "rustfmt", &["--version"])?);
                    toolchains.push(version(root, "cargo-clippy", &["--version"])?);
                    let path = protoc_bin_vendored::protoc_bin_path()?;
                    let output = Command::new(path).arg("--version").output()?;
                    ensure!(
                        output.status.success() && output.stdout.len() <= 4096,
                        "managed protoc version unavailable"
                    );
                    toolchains.push(ToolIdentity {
                        name: "managed-protoc".to_owned(),
                        version: String::from_utf8(output.stdout)?.trim().to_owned(),
                    });
                }
                Toolchain::Node => {
                    toolchains.push(version(root, "node", &["--version"])?);
                    toolchains.push(version(root, "npm", &["--version"])?);
                }
            }
        }
    }
    toolchains.sort_by(|left, right| left.name.cmp(&right.name));
    toolchains.dedup();
    // Values can contain secrets. Refuse reuse rather than publishing values or
    // hashes of arbitrary flags, configuration or injected instrumentation.
    let overridden = env::vars_os().any(|(key, _)| {
        let key = key.to_string_lossy();
        matches!(
            key.as_ref(),
            "RUSTFLAGS"
                | "CARGO_ENCODED_RUSTFLAGS"
                | "RUSTC"
                | "RUSTC_WRAPPER"
                | "RUSTC_WORKSPACE_WRAPPER"
                | "RUSTDOCFLAGS"
                | "CARGO_ENCODED_RUSTDOCFLAGS"
                | "PROTOC"
                | "NODE_OPTIONS"
                | "LD_PRELOAD"
        ) || key.starts_with("CARGO_PROFILE_")
            || key.starts_with("CARGO_TARGET_")
    });
    let cargo_home = env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|path| std::path::PathBuf::from(path).join(".cargo")));
    let external_config = cargo_home
        .is_some_and(|path| path.join("config").exists() || path.join("config.toml").exists());
    Ok(EnvironmentIdentity {
        os: env::consts::OS.to_owned(),
        architecture: env::consts::ARCH.to_owned(),
        toolchains,
        runtime: RuntimeIdentity {
            class: if definition.is_some() {
                EvidenceClass::Source
            } else {
                EvidenceClass::Unclassified
            },
            bindings: BTreeMap::new(),
            valid_for_seconds: None,
        },
        reusable: definition.is_some() && !overridden && !external_config,
    })
}

/// Source checks start without the loader path Cargo injects into `cargo run`.
/// Cargo owns loader paths for the binaries it subsequently builds and executes.
/// Unclassified commands retain their caller's environment.
pub(super) fn configure_source(command: &mut Command) {
    command.env_remove("LD_LIBRARY_PATH");
}
