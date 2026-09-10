//! A qualified, freshly observed provider connection. Availability grants no authority.
use crate::ApplicationError;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use veoveo_computers_runtime::OpenShellRuntime;

#[derive(Clone)]
struct Observed {
    runtime: OpenShellRuntime,
    at: Instant,
}
#[derive(Clone)]
pub struct RuntimeAccess(watch::Receiver<Option<Observed>>);
pub struct RuntimePublisher(watch::Sender<Option<Observed>>);
impl RuntimeAccess {
    pub fn channel() -> (RuntimePublisher, Self) {
        let (sender, receiver) = watch::channel(None);
        (RuntimePublisher(sender), Self(receiver))
    }
    pub fn unavailable() -> Self {
        Self::channel().1
    }
    pub(crate) fn current(&self) -> Result<OpenShellRuntime, ApplicationError> {
        self.0
            .has_changed()
            .map_err(|_| ApplicationError::Unavailable)?;
        let observed = self.0.borrow();
        observed
            .as_ref()
            .filter(|o| o.at <= Instant::now() && o.at.elapsed() <= Duration::from_secs(15))
            .map(|o| o.runtime.clone())
            .ok_or(ApplicationError::Unavailable)
    }
}
impl RuntimePublisher {
    /// Call only after a successful current provider readiness probe.
    pub fn available(&self, runtime: OpenShellRuntime) {
        self.0.send_replace(Some(Observed {
            runtime,
            at: Instant::now(),
        }));
    }
    pub fn unavailable(&self) {
        self.0.send_replace(None);
    }
}
