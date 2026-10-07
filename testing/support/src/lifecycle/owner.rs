//! Owner operation cancellation and one cleanup interval, shared with dispatcher D/G.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;

type Cleanup = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send>;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupKind {
    Remote,
    Browser,
    Container,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    kind: CleanupKind,
    owner: String,
    identity: String,
    observed_identity: Option<String>,
    settled: bool,
}
struct Entry {
    receipt: Receipt,
    action: Option<Cleanup>,
}
pub(crate) struct Active {
    pub cancelled: AtomicBool,
    notify: Notify,
    pub deadline: Instant,
    pub(crate) unix_end_ms: u128,
    pub grace: Duration,
    cleanup_end: Mutex<Option<Instant>>,
    entries: Mutex<Vec<Entry>>,
    first_cause: Mutex<Option<CancellationCause>>,
    pub(crate) groups: std::path::PathBuf,
}
#[derive(Clone)]
enum CancellationCause {
    Operator,
    Deadline,
    RequestedStop(super::teardown::StopRequest),
}
static ACTIVE: OnceLock<Mutex<Weak<Active>>> = OnceLock::new();
pub(crate) fn active() -> Option<Arc<Active>> {
    ACTIVE
        .get_or_init(|| Mutex::new(Weak::new()))
        .lock()
        .expect("owner state lock")
        .upgrade()
}
/// Deadline for an owning reconciliation operation; the active state stays private.
pub fn cleanup_deadline() -> Result<Instant> {
    Ok(active()
        .context("cleanup requires an active owner operation")?
        .cleanup_end())
}

pub fn has_active_owner() -> bool {
    active().is_some()
}

impl Active {
    fn cancel(&self, cause: CancellationCause) {
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            *self
                .first_cause
                .lock()
                .expect("owner first cancellation cause") = Some(cause);
            self.begin_cleanup();
            self.notify.notify_one();
        }
    }
    pub(crate) fn cleanup_end(&self) -> Instant {
        self.cleanup_end
            .lock()
            .expect("owner cleanup lock")
            .unwrap_or_else(|| (Instant::now() + self.grace).min(self.deadline + self.grace))
    }
    fn begin_cleanup(&self) -> Instant {
        *self
            .cleanup_end
            .lock()
            .expect("owner cleanup lock")
            .get_or_insert_with(|| (Instant::now() + self.grace).min(self.deadline + self.grace))
    }
}
/// One command's cancellation allowance, capped only by its captured owner.
pub(crate) struct CommandCleanup {
    owner: Option<Arc<Active>>,
    end: Mutex<Option<Instant>>,
}
impl CommandCleanup {
    pub(crate) fn new(owner: Option<Arc<Active>>) -> Arc<Self> {
        Arc::new(Self {
            owner,
            end: Mutex::new(None),
        })
    }
    pub(crate) fn check_effect(&self) -> Result<()> {
        if let Some(owner) = &self.owner {
            ensure!(
                !owner.cancelled.load(Ordering::Acquire) && Instant::now() < owner.deadline,
                "original command owner cancelled or execution deadline expired"
            );
        }
        Ok(())
    }
    pub(crate) fn end(&self) -> Instant {
        let owner_cap = self.owner.as_ref().map(|owner| owner.cleanup_end());
        let mut stored = self.end.lock().expect("command cleanup lock");
        let end = stored.get_or_insert_with(|| {
            let allowance = Instant::now() + Duration::from_secs(1);
            owner_cap.map_or(allowance, |cap| allowance.min(cap))
        });
        // A running owner's cleanup cap advances with time. The first command
        // cancellation captures that cap; later owner observations may only shorten it.
        if let Some(cap) = owner_cap {
            *end = (*end).min(cap);
        }
        *end
    }
}
#[derive(Clone)]
pub struct CleanupRegistration {
    scope: Weak<Active>,
    index: usize,
}
impl CleanupRegistration {
    /// Retain an owning protocol's acknowledged identity alongside dispatch intent.
    pub fn observed_identity(&self, identity: &str) -> Result<()> {
        ensure!(
            !identity.is_empty() && identity.len() <= 2048,
            "invalid observed cleanup identity"
        );
        let scope = self
            .scope
            .upgrade()
            .context("cleanup owner is no longer active")?;
        let mut entries = scope.entries.lock().expect("owner cleanup entries");
        let entry = entries
            .get_mut(self.index)
            .context("missing cleanup registration")?;
        ensure!(
            !entry.receipt.settled,
            "cleanup identity is already settled"
        );
        if let Some(previous) = &entry.receipt.observed_identity {
            ensure!(previous == identity, "observed cleanup identity changed");
        } else {
            entry.receipt.observed_identity = Some(identity.to_owned());
        }
        Ok(())
    }
    /// Called only after the owning protocol proves terminal settlement.
    pub fn settled(&self) -> Result<()> {
        let scope = self
            .scope
            .upgrade()
            .context("cleanup owner is no longer active")?;
        let mut entries = scope.entries.lock().expect("owner cleanup entries");
        let entry = entries
            .get_mut(self.index)
            .context("missing cleanup registration")?;
        entry.receipt.settled = true;
        entry.action = None;
        Ok(())
    }
}
/// Called immediately before admitting a new local or remote effect.
pub fn check_effect() -> Result<()> {
    if let Some(scope) = active() {
        ensure!(
            !scope.cancelled.load(Ordering::Acquire) && Instant::now() < scope.deadline,
            "owner cancelled or execution deadline expired; no new effects admitted"
        );
    }
    Ok(())
}
/// Owner identity and cancellation action are retained until authoritative settlement.
/// The owner supplies the maintained protocol/client operation; shared support does
/// not interpret domain identities or turn unknown remote outcomes into success.
pub fn register_cleanup<F, Fut>(
    kind: CleanupKind,
    owner: &str,
    identity: &str,
    action: F,
) -> Result<CleanupRegistration>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    check_effect()?;
    let scope = active().context("cleanup registration requires an active owner operation")?;
    ensure!(
        !owner.is_empty() && !identity.is_empty() && identity.len() <= 2048,
        "invalid owner cleanup identity"
    );
    let mut entries = scope.entries.lock().expect("owner cleanup entries");
    ensure!(
        !scope.cancelled.load(Ordering::Acquire),
        "owner cancelled during cleanup registration"
    );
    let index = entries.len();
    entries.push(Entry {
        receipt: Receipt {
            kind,
            owner: owner.to_owned(),
            identity: identity.to_owned(),
            observed_identity: None,
            settled: false,
        },
        action: Some(Box::new(move || Box::pin(action()))),
    });
    Ok(CleanupRegistration {
        scope: Arc::downgrade(&scope),
        index,
    })
}
fn persist_unresolved(scope: &Active) -> Result<()> {
    let receipts: Vec<_> = scope
        .entries
        .lock()
        .expect("owner cleanup entries")
        .iter()
        .filter(|entry| !entry.receipt.settled)
        .map(|entry| entry.receipt.clone())
        .collect();
    if receipts.is_empty() {
        return Ok(());
    }
    let path = scope
        .groups
        .join(format!(".unresolved-owner-{}.json", std::process::id()));
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&serde_json::to_vec(&receipts)?)?;
    file.sync_all()?;
    anyhow::bail!("unresolved owner cleanup identities retained in private ownership lease")
}
/// All actual owner entrypoints use this wrapper. Signal observation is installed
/// before operation polling and repeated signals cannot extend cleanup.
pub async fn run<F, T>(operation: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    let unix_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let end_ms = match std::env::var("VEOVEO_SMOKE_DEADLINE_UNIX_MS") {
        Ok(value) => value.parse::<u128>().context("invalid owner deadline")?,
        Err(std::env::VarError::NotPresent) => unix_ms + 300_000,
        Err(error) => return Err(error.into()),
    };
    ensure!(end_ms > unix_ms, "owner execution deadline expired");
    let grace = Duration::from_secs(match std::env::var("VEOVEO_SMOKE_CLEANUP_SECONDS") {
        Ok(value) => value
            .parse::<u64>()
            .context("invalid owner cleanup budget")?,
        Err(std::env::VarError::NotPresent) => 20,
        Err(error) => return Err(error.into()),
    });
    ensure!(!grace.is_zero(), "owner cleanup budget must be positive");
    let groups = match std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS") {
        Some(root) => std::path::PathBuf::from(root),
        None => {
            use std::os::unix::fs::DirBuilderExt;
            let root = std::env::temp_dir().join(format!("veoveo-owner-{}", uuid::Uuid::now_v7()));
            std::fs::DirBuilder::new().mode(0o700).create(&root)?;
            root
        }
    };
    let scope = Arc::new(Active {
        cancelled: AtomicBool::new(false),
        notify: Notify::new(),
        unix_end_ms: end_ms,
        deadline: Instant::now() + Duration::from_millis(u64::try_from(end_ms - unix_ms)?),
        grace,
        cleanup_end: Mutex::new(None),
        entries: Mutex::new(Vec::new()),
        first_cause: Mutex::new(None),
        groups,
    });
    {
        let mut current = ACTIVE
            .get_or_init(|| Mutex::new(Weak::new()))
            .lock()
            .expect("owner state lock");
        ensure!(
            current.upgrade().is_none(),
            "overlapping owner operations are unsupported"
        );
        *current = Arc::downgrade(&scope);
    }
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let signal_scope = Arc::clone(&scope);
    let signals = tokio::spawn(async move {
        let mut stop_requests = tokio::time::interval(Duration::from_millis(10));
        loop {
            tokio::select! {
                biased;
                _ = interrupt.recv() => signal_scope.cancel(CancellationCause::Operator),
                _ = terminate.recv() => signal_scope.cancel(CancellationCause::Operator),
                _ = tokio::time::sleep_until(signal_scope.deadline.into()), if !signal_scope.cancelled.load(Ordering::Acquire) => signal_scope.cancel(CancellationCause::Deadline),
                _ = stop_requests.tick() => {
                    if let Ok(request)=super::teardown::current_request(&signal_scope.groups) {
                        signal_scope.cancel(CancellationCause::RequestedStop(request));
                    }
                }
            }
        }
    });
    let result = tokio::select! {
        result = operation => result,
        _ = scope.notify.notified() => Err(anyhow::anyhow!("owner operation cancelled; cleanup/reconciliation required")),
        _ = tokio::time::sleep_until(scope.deadline.into()) => {
            scope.cancel(CancellationCause::Deadline); Err(anyhow::anyhow!("owner execution deadline expired; cleanup/reconciliation required"))
        }
    };
    let end = scope.begin_cleanup();
    let actions: Vec<_> = scope
        .entries
        .lock()
        .expect("owner cleanup entries")
        .iter_mut()
        .enumerate()
        .filter_map(|(index, entry)| entry.action.take().map(|action| (index, action)))
        .collect();
    for (index, action) in actions.into_iter().rev() {
        let remaining = end.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        if matches!(tokio::time::timeout(remaining, action()).await, Ok(Ok(()))) {
            scope.entries.lock().expect("owner cleanup entries")[index]
                .receipt
                .settled = true;
        }
    }
    signals.abort();
    let _ = signals.await;
    *ACTIVE
        .get_or_init(|| Mutex::new(Weak::new()))
        .lock()
        .expect("owner state lock") = Weak::new();
    persist_unresolved(&scope)?;
    let cause = scope
        .first_cause
        .lock()
        .expect("owner first cancellation cause")
        .clone();
    if let Some(CancellationCause::RequestedStop(request)) = cause {
        let count = scope.entries.lock().expect("owner cleanup entries").len();
        super::teardown::settled(&scope.groups, &request, count)?;
    }

    ensure!(
        !scope.cancelled.load(Ordering::Acquire),
        "owner cancellation cannot become success"
    );
    result
}

#[cfg(test)]
pub(crate) fn test_scope(groups: std::path::PathBuf, grace: Duration) -> Arc<Active> {
    Arc::new(Active {
        cancelled: AtomicBool::new(false),
        notify: Notify::new(),
        unix_end_ms: 0,
        deadline: Instant::now() + Duration::from_secs(10),
        grace,
        cleanup_end: Mutex::new(None),
        entries: Mutex::new(Vec::new()),
        first_cause: Mutex::new(None),
        groups,
    })
}
#[cfg(test)]
pub(crate) fn test_cancel(scope: &Active) {
    scope.cancel(CancellationCause::Operator);
}
#[cfg(test)]
pub(crate) fn test_activate(scope: Option<&Arc<Active>>) {
    *ACTIVE
        .get_or_init(|| Mutex::new(Weak::new()))
        .lock()
        .unwrap() = scope.map_or_else(Weak::new, Arc::downgrade);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intended_fixture_stop_settles_owned_cleanup_without_scenario_success() {
        const CHILD: &str = "VEOVEO_TEST_INTENDED_FIXTURE_STOP";
        const MODE: &str = "VEOVEO_TEST_FIXTURE_STOP_MODE";
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let mode = std::env::var(MODE).unwrap();
            let unresolved = mode == "unresolved";
            let result: Result<()> = runtime.block_on(run(async {
                let marker = root.join("settled");
                let registration = register_cleanup(
                    CleanupKind::Remote,
                    "fake_provider",
                    "operation-1",
                    move || async move {
                        std::fs::write(marker, b"settled")?;
                        ensure!(
                            !unresolved,
                            "fake provider retained an unresolved operation"
                        );
                        Ok(())
                    },
                )?;
                registration.observed_identity("acknowledged-operation-1")?;
                ensure!(
                    registration
                        .observed_identity("different-operation")
                        .is_err(),
                    "cleanup identity rebind was admitted"
                );
                std::fs::write(root.join("ready"), b"ready")?;
                if mode == "deadline" {
                    // Exercise the active owner's first-cause transition. The
                    // independent execution-deadline test covers elapsed D.
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    active()
                        .context("missing actual fixture owner")?
                        .cancel(CancellationCause::Deadline);
                }
                std::future::pending::<Result<()>>().await
            }));
            // Teardown settlement never changes the cancelled scenario result.
            assert!(result.is_err());
            assert_eq!(std::fs::read(root.join("settled")).unwrap(), b"settled");
            if mode != "requested" {
                // A successful test framework exit must not disguise scenario
                // cancellation/refused cleanup as a successful fixture operation.
                std::process::exit(1);
            }
            return;
        }
        for mode in ["requested", "operator", "deadline", "unresolved"] {
            let root = tempfile::tempdir().unwrap();
            runtime.block_on(run(async {
                let mut command = std::process::Command::new(std::env::current_exe()?);
                command.args(["--exact", "lifecycle::owner::tests::intended_fixture_stop_settles_owned_cleanup_without_scenario_success", "--nocapture"])
                    .env(CHILD, root.path()).env(MODE, mode);
                let mut child = crate::process::ChildGuard::from_command(command)?.with_owner_scope();
                let ready_end = Instant::now() + Duration::from_secs(3);
                while !root.path().join("ready").exists() {
                    check_effect()?;
                    ensure!(Instant::now() < ready_end, "fixture did not become ready");
                    ensure!(child.try_wait()?.is_none(), "fixture exited before its cleanup registration");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                if mode == "operator" {
                    nix::sys::signal::killpg(nix::unistd::Pid::from_raw(child.child_pid() as i32), nix::sys::signal::Signal::SIGINT)?;
                }
                if matches!(mode, "operator" | "deadline") {
                    let settled_end = Instant::now() + Duration::from_secs(2);
                    while !root.path().join("settled").exists() {
                        ensure!(Instant::now() < settled_end, "prior cancellation did not execute registered cleanup");
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
                let result = child.drain(Duration::from_secs(2)).await;
                if mode == "requested" {
                    result?;
                } else {
                    ensure!(result.is_err(), "{mode} cancellation/refusal became successful teardown");
                }
                ensure!(std::fs::read(root.path().join("settled"))? == b"settled", "fixture cleanup action was not observed");
                Ok(())
            })).unwrap();
        }
    }

    #[test]
    fn repeated_cancellation_does_not_extend_cleanup_or_admit_new_effects() {
        let scope = Active {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
            unix_end_ms: 0,
            deadline: Instant::now() + Duration::from_secs(10),
            grace: Duration::from_millis(10),
            cleanup_end: Mutex::new(None),
            entries: Mutex::new(Vec::new()),
            first_cause: Mutex::new(None),
            groups: std::env::temp_dir(),
        };
        scope.cancel(CancellationCause::Deadline);
        let first = scope.cleanup_end();
        scope.cancel(CancellationCause::Operator);
        let stop: super::super::teardown::StopRequest = serde_json::from_value(serde_json::json!({
            "format": "veoveo.ai/smoke-stop/v1",
            "pid": 42,
            "startTicks": 1,
            "invocationDigest": veoveo_types::Sha256Digest::from_bytes([1; 32]),
            "stopId": uuid::Uuid::now_v7(),
        }))
        .unwrap();
        scope.cancel(CancellationCause::RequestedStop(stop));
        assert!(matches!(
            *scope.first_cause.lock().unwrap(),
            Some(CancellationCause::Deadline)
        ));
        assert_eq!(first, scope.cleanup_end());
        assert!(scope.cancelled.load(Ordering::Acquire));
    }
}
