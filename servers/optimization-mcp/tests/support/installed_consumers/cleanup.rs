//! Actual consuming SDK close futures survive cancellation of the read operation.
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    fs,
    future::Future,
    io::Write,
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Instant,
};
use veoveo_testing_support::lifecycle::owner::{self, CleanupKind, CleanupRegistration};

#[derive(Clone)]
pub struct Journal(Arc<Mutex<fs::File>>);
impl Journal {
    pub fn create(path: &Path) -> Result<Self> {
        ensure!(
            path.is_absolute(),
            "report requires absolute create-new path"
        );
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Ok(Self(Arc::new(Mutex::new(options.open(path)?))))
    }
    pub fn append(&self, record: &impl Serialize) -> Result<()> {
        let mut file = self.0.lock().expect("consumer journal");
        serde_json::to_writer(&mut *file, record)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }
}
#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    NotAcquired,
    Retained,
    Closing,
    Passed,
    Failed,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Unfinished,
    Passed,
    Failed,
}
type CloseFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
struct Slot {
    state: State,
    future: Option<CloseFuture>,
    deadline: Option<Instant>,
}
pub struct Cleanup {
    slots: tokio::sync::Mutex<[Slot; 2]>,
    operation: Mutex<Operation>,
    journal: Journal,
}
impl Cleanup {
    pub fn register(journal: Journal) -> Result<(Arc<Self>, CleanupRegistration)> {
        let cleanup = Arc::new(Self {
            slots: tokio::sync::Mutex::new(std::array::from_fn(|_| Slot {
                state: State::NotAcquired,
                future: None,
                deadline: None,
            })),
            operation: Mutex::new(Operation::Unfinished),
            journal,
        });
        let retained = Arc::clone(&cleanup);
        let registration = owner::register_cleanup(
            CleanupKind::Remote,
            "optimization_installed_read_clients",
            "primary_and_alternate",
            move || async move {
                let before = retained.record().await;
                let close = retained.close().await;
                let after = retained.record().await;
                before?;
                after?;
                close
            },
        )?;
        Ok((cleanup, registration))
    }
    // Lock the retained slot before connection acquisition. Dropping an acquisition
    // releases the guard; a returned handle is stored synchronously before any await.
    pub async fn admission(&self) -> Admission<'_> {
        Admission(self.slots.lock().await)
    }
    pub fn finish(&self, passed: bool) {
        *self.operation.lock().expect("consumer operation") = if passed {
            Operation::Passed
        } else {
            Operation::Failed
        };
    }
    pub async fn close(&self) -> Result<()> {
        // Capture ONE owner deadline for both clients before the first close. A later
        // owner callback may shorten each cap, never renew a failed/interrupted close.
        let end = owner::cleanup_deadline()?;
        let mut slots = self.slots.lock().await;
        for slot in slots.iter_mut() {
            slot.deadline = Some(slot.deadline.map_or(end, |d| d.min(end)));
        }
        for slot in slots.iter_mut() {
            match slot.state {
                State::NotAcquired | State::Passed | State::Failed => continue,
                State::Retained | State::Closing => {}
            }
            slot.state = State::Closing;
            let passed = if let Some(future) = slot.future.as_mut() {
                matches!(
                    tokio::time::timeout_at(slot.deadline.unwrap().into(), future).await,
                    Ok(Ok(()))
                )
            } else {
                false
            };
            slot.state = if passed { State::Passed } else { State::Failed };
            slot.future = None;
        }
        ensure!(
            !slots.iter().any(|s| s.state == State::Failed),
            "original SDK close failed or exhausted owner deadline"
        );
        Ok(())
    }
    pub async fn record(&self) -> Result<()> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Record {
            phase: &'static str,
            operation: Operation,
            owner_cancelled: bool,
            clients: [State; 2],
        }
        let slots = self.slots.lock().await;
        self.journal.append(&Record {
            phase: "cleanup",
            operation: *self.operation.lock().expect("consumer operation"),
            owner_cancelled: owner::check_effect().is_err(),
            clients: [slots[0].state, slots[1].state],
        })
    }
}

pub struct Admission<'a>(tokio::sync::MutexGuard<'a, [Slot; 2]>);
impl Admission<'_> {
    pub fn retain(
        &mut self,
        index: usize,
        future: impl Future<Output = Result<()>> + Send + 'static,
    ) {
        assert!(self.0[index].state == State::NotAcquired);
        self.0[index].future = Some(Box::pin(future));
        self.0[index].state = State::Retained;
    }
}
