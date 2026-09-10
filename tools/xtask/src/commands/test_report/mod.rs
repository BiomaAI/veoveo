//! Per-check immutable evidence. Commands and harnesses retain execution ownership.
mod cargo_inputs;
mod catalog;
mod environment;
mod inputs;
mod model;
mod report;
mod storage;
#[cfg(test)]
mod tests;

use std::{
    env,
    ffi::OsString,
    path::Path,
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use chrono::Utc;
use uuid::Uuid;

use crate::{context::RepositoryContext, process};
use model::{CommandIdentity, InputScope, Outcome, RECEIPT_SCHEMA, Receipt, SourceProvenance};

pub(crate) use report::{show, verify};

pub(crate) fn run(
    repository: &RepositoryContext,
    name: &str,
    arguments: &[OsString],
) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 64
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
        "check name must contain 1-64 lowercase ASCII letters, digits, or hyphens"
    );
    ensure!(
        !arguments.is_empty(),
        "a check command is required after --"
    );
    let root = repository.root();
    let definition = catalog::lookup(root, arguments)?;
    let run_id = Uuid::new_v4();
    let (command, check_id, scope) = match &definition {
        Some(definition) => (
            CommandIdentity::Admitted {
                arguments: definition.arguments.clone(),
            },
            catalog::check_id(&definition.arguments)?,
            definition.inputs.clone(),
        ),
        None => (
            CommandIdentity::Opaque {
                program: "unclassified invocation".to_owned(),
            },
            format!("unclassified:{run_id}"),
            InputScope::Repository,
        ),
    };
    let before = inputs::snapshot(root, &scope)?;
    let observed_environment = environment::observe(root, definition.as_ref())?;
    println!(
        "Evidence inputs: {} files, {}; reuse {}",
        before.files.len(),
        before.digest,
        if observed_environment.reusable {
            "admitted"
        } else {
            "unqualified"
        }
    );
    let provenance = provenance(root)?;
    let started_at = Utc::now();
    let started = Instant::now();
    let result = execute(root, arguments, definition.is_some());
    let duration_millis = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let finished_at = Utc::now();
    let unchanged = inputs::snapshot(root, &scope).is_ok_and(|after| after == before)
        && environment::observe(root, definition.as_ref())
            .is_ok_and(|after| after == observed_environment);
    let outcome = if !unchanged {
        Outcome::InputsChanged
    } else if result.as_ref().is_ok_and(|status| status.success()) {
        Outcome::Passed
    } else {
        Outcome::Failed
    };
    let receipt = Receipt {
        schema_version: RECEIPT_SCHEMA.to_owned(),
        run_id,
        name: name.to_owned(),
        check_id,
        command,
        provenance,
        inputs: before,
        environment: observed_environment,
        started_at,
        finished_at,
        duration_millis,
        outcome,
        exit_code: result.as_ref().ok().and_then(|status| status.code()),
        diagnostics: None,
    };
    storage::publish(root, &receipt)?;
    println!("Recorded {} ({outcome:?})", model::INDEX_PATH);
    ensure!(
        outcome == Outcome::Passed,
        "local check did not qualify; the attempt was recorded"
    );
    Ok(())
}

fn provenance(root: &Path) -> Result<SourceProvenance> {
    let revision = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(root)
        .output()?;
    let revision = revision
        .status
        .success()
        .then(|| String::from_utf8_lossy(&revision.stdout).trim().to_owned());
    let status = process::output(
        "git",
        ["status", "--porcelain", "--untracked-files=normal"],
        Some(root),
    )?;
    Ok(SourceProvenance {
        revision,
        dirty: !status.stdout.is_empty(),
    })
}

fn execute(
    root: &Path,
    arguments: &[OsString],
    admitted: bool,
) -> Result<std::process::ExitStatus> {
    let protoc = env::var_os("PROTOC").map(Ok).unwrap_or_else(|| {
        protoc_bin_vendored::protoc_bin_path().map(std::path::PathBuf::into_os_string)
    })?;
    let mut command = Command::new(&arguments[0]);
    command
        .args(&arguments[1..])
        .current_dir(root)
        .env(
            "CARGO_BUILD_JOBS",
            env::var_os("CARGO_BUILD_JOBS")
                .unwrap_or_else(|| process::DEFAULT_CARGO_BUILD_JOBS.into()),
        )
        .env("PROTOC", protoc)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    process::remove_parent_cargo_package_environment(&mut command);
    if admitted {
        environment::configure_source(&mut command);
    }
    command.status().context("starting local check")
}
