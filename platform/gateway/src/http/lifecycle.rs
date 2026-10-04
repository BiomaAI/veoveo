use parking_lot::Mutex;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use tokio_util::{
    sync::CancellationToken,
    task::{TaskTracker, task_tracker::TaskTrackerToken},
};

pub const MODULE_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

struct ScopeState {
    open: bool,
    tracker: TaskTracker,
}
struct ScopeInner {
    state: Mutex<ScopeState>,
    stop: CancellationToken,
}

/// A synchronized admission fence shared by requests and all module workers.
#[derive(Clone)]
pub struct ModuleTaskScope {
    inner: Arc<ScopeInner>,
}

#[derive(Debug, thiserror::Error)]
#[error("gateway module is shutting down")]
pub struct ModuleScopeClosed;

pub struct ModuleTaskPermit {
    _token: TaskTrackerToken,
    stop: CancellationToken,
}
impl ModuleTaskPermit {
    pub fn cancellation_token(&self) -> CancellationToken {
        self.stop.clone()
    }
    pub(crate) fn handoff<F>(self, future: F) -> JoinHandle<(F::Output, Self)>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::spawn(async move { (future.await, self) })
    }
    /// A reservation owns an upgrade callback even before that callback starts.
    pub fn spawn<F>(self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::spawn(async move {
            let _permit = self;
            future.await
        })
    }
}
impl ModuleTaskScope {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ScopeInner {
                state: Mutex::new(ScopeState {
                    open: true,
                    tracker: TaskTracker::new(),
                }),
                stop: CancellationToken::new(),
            }),
        }
    }
    pub fn cancellation_token(&self) -> CancellationToken {
        self.inner.stop.clone()
    }
    pub fn reserve(&self) -> Result<ModuleTaskPermit, ModuleScopeClosed> {
        let state = self.inner.state.lock();
        if !state.open {
            return Err(ModuleScopeClosed);
        }
        Ok(ModuleTaskPermit {
            _token: state.tracker.token(),
            stop: self.inner.stop.clone(),
        })
    }
    pub fn spawn<F>(&self, future: F) -> Result<JoinHandle<F::Output>, ModuleScopeClosed>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        Ok(self.reserve()?.spawn(future))
    }
    pub fn close(&self) {
        let mut state = self.inner.state.lock();
        state.open = false;
        state.tracker.close();
    }
    pub fn cancel(&self) {
        self.close();
        self.inner.stop.cancel();
    }
    pub async fn wait(&self) {
        let tracker = self.inner.state.lock().tracker.clone();
        tracker.wait().await;
    }
}
impl Default for ModuleTaskScope {
    fn default() -> Self {
        Self::new()
    }
}

/// The process retains this supervisor before starting construction, including
/// when the build future is dropped. Cleanup outcomes remain observable.
type CleanupResult = Result<(), String>;
type CleanupReservation = (
    usize,
    tokio::sync::watch::Sender<Option<CleanupResult>>,
    tokio::sync::watch::Receiver<Option<CleanupResult>>,
);
struct CleanupRecord {
    completion: tokio::sync::watch::Receiver<Option<CleanupResult>>,
    task: Option<JoinHandle<()>>,
}
#[derive(Default)]
struct SupervisorState {
    closed: bool,
    records: Vec<CleanupRecord>,
}
#[derive(Clone, Default)]
pub struct ModuleCleanupSupervisor {
    state: Arc<Mutex<SupervisorState>>,
}
impl ModuleCleanupSupervisor {
    fn reserve(&self) -> anyhow::Result<CleanupReservation> {
        let mut state = self.state.lock();
        if state.closed {
            anyhow::bail!("gateway module cleanup supervisor is closed");
        }
        let (sender, receiver) = tokio::sync::watch::channel(None);
        let index = state.records.len();
        state.records.push(CleanupRecord {
            completion: receiver.clone(),
            task: None,
        });
        Ok((index, sender, receiver))
    }
    /// Close construction admission and observe every reserved cleanup slot.
    /// Receivers stay in the supervisor if this awaiter is cancelled.
    pub async fn wait(&self) -> anyhow::Result<()> {
        let records = {
            let mut state = self.state.lock();
            state.closed = true;
            state
                .records
                .iter()
                .map(|record| record.completion.clone())
                .collect::<Vec<_>>()
        };
        let mut errors = vec![];
        for mut receiver in records {
            loop {
                if let Some(result) = receiver.borrow().clone() {
                    if let Err(error) = result {
                        errors.push(error);
                    }
                    break;
                }
                if receiver.changed().await.is_err() {
                    errors.push("module cleanup owner ended without settlement".into());
                    break;
                }
            }
        }
        if !errors.is_empty() {
            anyhow::bail!("module cleanup unresolved: {}", errors.join("; "));
        }
        Ok(())
    }
}

pub(crate) struct CleanupGuard {
    pub scopes: Vec<ModuleTaskScope>,
    pub supervisor: ModuleCleanupSupervisor,
    completion: tokio::sync::watch::Receiver<Option<CleanupResult>>,
    sender: tokio::sync::watch::Sender<Option<CleanupResult>>,
    record: usize,
    started: bool,
    pub deadline: Arc<Mutex<Option<tokio::time::Instant>>>,
}
impl CleanupGuard {
    pub fn new(supervisor: ModuleCleanupSupervisor) -> anyhow::Result<Self> {
        let (record, sender, completion) = supervisor.reserve()?;
        Ok(Self {
            scopes: vec![],
            supervisor,
            completion,
            sender,
            record,
            started: false,
            deadline: Arc::new(Mutex::new(None)),
        })
    }
    fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        if self.scopes.is_empty() {
            self.sender.send_replace(Some(Ok(())));
            return;
        }
        for scope in &self.scopes {
            scope.cancel();
        }
        let deadline = *self
            .deadline
            .lock()
            .get_or_insert_with(|| tokio::time::Instant::now() + MODULE_DRAIN_TIMEOUT);
        let scopes = std::mem::take(&mut self.scopes);
        let sender = self.sender.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let task = runtime.spawn(async move {
                let result = tokio::time::timeout_at(
                    deadline,
                    futures::future::join_all(scopes.iter().map(ModuleTaskScope::wait)),
                )
                .await
                .map(|_| ())
                .map_err(|_| {
                    "gateway module cleanup exceeded 30 seconds; work remains unresolved".to_owned()
                });
                sender.send_replace(Some(result.clone()));
            });
            self.supervisor.state.lock().records[self.record].task = Some(task);
        } else {
            sender.send_replace(Some(Err(
                "gateway module cleanup requires a live Tokio runtime; work remains unresolved"
                    .into(),
            )));
        }
    }
    pub async fn finish(&mut self) -> anyhow::Result<()> {
        self.start();
        let receiver = &mut self.completion;
        loop {
            if let Some(result) = receiver.borrow().clone() {
                return result.map_err(anyhow::Error::msg);
            }
            receiver.changed().await.map_err(|_| {
                anyhow::anyhow!("module cleanup supervisor ended without settlement")
            })?;
        }
    }
}
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        self.start();
    }
}

#[derive(Clone)]
pub struct ModuleShutdownSignal {
    pub(crate) scopes: Vec<ModuleTaskScope>,
    pub(crate) deadline: Arc<Mutex<Option<tokio::time::Instant>>>,
}
impl ModuleShutdownSignal {
    /// Drain admitted work before closing audit delivery, using the same shutdown deadline.
    pub async fn drain(&self) -> anyhow::Result<()> {
        self.cancel();
        let deadline = self
            .deadline
            .lock()
            .expect("cancel establishes module deadline");
        tokio::time::timeout_at(deadline, async {
            for scope in &self.scopes {
                scope.wait().await;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("gateway module cleanup exceeded shared shutdown deadline"))
    }
    pub fn cancel(&self) {
        self.deadline
            .lock()
            .get_or_insert_with(|| tokio::time::Instant::now() + MODULE_DRAIN_TIMEOUT);
        for scope in &self.scopes {
            scope.cancel();
        }
    }
}
