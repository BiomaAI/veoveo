//! Command-level timing, including work outside an individual BuildKit solve.
//! The synchronous command owns a scoped recorder; nested functions emit spans.
use std::{
    cell::RefCell,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use tempfile::NamedTempFile;
use veoveo_image_build_control::CpuSnapshot;

use crate::context::RepositoryContext;

thread_local! {
    static ACTIVE: RefCell<Option<Session>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy)]
pub(crate) struct CommandClock {
    started: Instant,
    wall: SystemTime,
}

impl CommandClock {
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            wall: SystemTime::now(),
        }
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Phase {
    SourcePreparation,
    RegistryPreflight,
    BuilderSetup,
    Planning,
    Solve,
    ParentResolution,
    ParentPublication,
    ManifestInspection,
    ReceiptWrite,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Timing {
    phase: Phase,
    started_offset_millis: u64,
    elapsed_millis: u64,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueWaits {
    source_lock_millis: u64,
    builder_lock_millis: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Source {
    path: PathBuf,
    revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CpuDelta {
    usage_micros: u64,
    periods: u64,
    throttled_periods: u64,
    throttled_micros: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Outcome {
    Running,
    Succeeded,
    Failed,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    schema_version: &'static str,
    operation: String,
    started_at_unix_millis: u64,
    elapsed_millis: u64,
    outcome: Outcome,
    error: Option<String>,
    queue_waits: QueueWaits,
    phases: Vec<Timing>,
    sources: Vec<Source>,
    solve_evidence: Vec<PathBuf>,
    cpu_deltas: Vec<CpuDelta>,
    diagnostics: Vec<String>,
}

struct Session {
    clock: CommandClock,
    record: Record,
}

pub(crate) fn record<T>(
    repository: &RepositoryContext,
    operation: &str,
    clock: CommandClock,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    ensure!(
        ACTIVE.with(|active| active.borrow().is_none()),
        "nested command timing recorder"
    );
    let wall = clock
        .wall
        .duration_since(UNIX_EPOCH)
        .context("clock is before the Unix epoch")?;
    let directory = repository
        .root()
        .join("target/veoveo-xtask/operations")
        .join(format!(
            "{}-{operation}-{}",
            wall.as_nanos(),
            std::process::id()
        ));
    fs::create_dir_all(&directory).context("creating command timing directory")?;
    let path = directory.join("command.json");
    let initial = Record {
        schema_version: "veoveo.io/image-command/v1",
        operation: operation.to_owned(),
        started_at_unix_millis: millis(wall),
        elapsed_millis: millis(clock.started.elapsed()),
        outcome: Outcome::Running,
        error: None,
        queue_waits: QueueWaits::default(),
        phases: Vec::new(),
        sources: Vec::new(),
        solve_evidence: Vec::new(),
        cpu_deltas: Vec::new(),
        diagnostics: Vec::new(),
    };
    write_record(&path, &initial)?;
    ACTIVE.with(|active| {
        *active.borrow_mut() = Some(Session {
            clock,
            record: initial,
        })
    });
    let result = action();
    let mut session = ACTIVE
        .with(|active| active.borrow_mut().take())
        .context("command timing recorder disappeared")?;
    session.record.elapsed_millis = millis(clock.started.elapsed());
    session.record.outcome = if result.is_ok() {
        Outcome::Succeeded
    } else {
        Outcome::Failed
    };
    session.record.error = result.as_ref().err().map(|error| format!("{error:#}"));
    let written = write_record(&path, &session.record);
    eprintln!(
        "Command timing: {} ({} ms)",
        path.display(),
        session.record.elapsed_millis
    );
    match (result, written) {
        (Err(error), Err(record_error)) => Err(error.context(format!(
            "also failed to save command timing: {record_error:#}"
        ))),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

fn write_record(path: &Path, record: &Record) -> Result<()> {
    let mut file = NamedTempFile::new_in(path.parent().context("timing path has no parent")?)?;
    serde_json::to_writer_pretty(&mut file, record)?;
    file.write_all(b"\n")?;
    file.persist(path)
        .map_err(|error| error.error)
        .context("saving command timing")?;
    Ok(())
}

pub(crate) struct Span {
    phase: Phase,
    started: Instant,
}

pub(crate) fn span(phase: Phase) -> Span {
    Span {
        phase,
        started: Instant::now(),
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            if let Some(session) = active.borrow_mut().as_mut() {
                session.record.phases.push(Timing {
                    phase: self.phase,
                    started_offset_millis: millis(
                        self.started
                            .saturating_duration_since(session.clock.started),
                    ),
                    elapsed_millis: millis(self.started.elapsed()),
                });
            }
        });
    }
}

pub(crate) fn source_lock_wait(duration: Duration) {
    update(|record| record.queue_waits.source_lock_millis += millis(duration));
}

pub(crate) fn builder_lock_wait(duration: Duration) {
    update(|record| record.queue_waits.builder_lock_millis += millis(duration));
}

pub(crate) fn source(path: &Path, revision: &str) {
    update(|record| {
        record.sources.push(Source {
            path: path.to_owned(),
            revision: revision.to_owned(),
        })
    });
}

pub(crate) fn solve_evidence(path: &Path) {
    update(|record| record.solve_evidence.push(path.to_owned()));
}

pub(crate) fn cpu_delta(after: Result<CpuSnapshot>, before: Result<CpuSnapshot>) {
    update(|record| {
        match after.and_then(|after| {
            before.and_then(|before| {
                after
                    .checked_delta(before)
                    .context("builder CPU counters reset during solve")
            })
        }) {
            Ok(delta) => record.cpu_deltas.push(CpuDelta {
                usage_micros: delta.usage_us,
                periods: delta.periods,
                throttled_periods: delta.throttled_periods,
                throttled_micros: delta.throttled_us,
            }),
            Err(error) => record
                .diagnostics
                .push(format!("CPU telemetry unavailable: {error:#}")),
        }
    });
}

fn update(action: impl FnOnce(&mut Record)) {
    ACTIVE.with(|active| {
        if let Some(session) = active.borrow_mut().as_mut() {
            action(&mut session.record);
        }
    });
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_failures_before_a_solve_and_resets_the_recorder() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
        let repository = RepositoryContext::discover(root.path()).unwrap();
        let result: Result<()> = record(&repository, "stage", CommandClock::start(), || {
            let _span = span(Phase::SourcePreparation);
            source_lock_wait(Duration::from_millis(23));
            anyhow::bail!("missing source revision")
        });
        assert!(result.is_err());
        let operations = root.path().join("target/veoveo-xtask/operations");
        let directory = fs::read_dir(operations)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("command.json")).unwrap()).unwrap();
        assert_eq!(record["outcome"], "failed");
        assert_eq!(record["error"], "missing source revision");
        assert_eq!(record["queueWaits"]["sourceLockMillis"], 23);
        assert_eq!(record["phases"][0]["phase"], "source-preparation");
        assert!(record["solveEvidence"].as_array().unwrap().is_empty());
        assert!(ACTIVE.with(|active| active.borrow().is_none()));
    }
}
