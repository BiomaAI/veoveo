use super::super::access_events::Listener;
use crate::Application;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::{Notify, watch};
use uuid::Uuid;
use veoveo_computers::api::{TerminalLease, TerminalLeaseKind};
use veoveo_computers::session_grants::SessionGrantHandle;
use veoveo_computers_runtime::LeaseAuthority;
use veoveo_platform_store::RecordId;

pub(super) struct Activity {
    input: AtomicBool,
    wake: Notify,
    renewed: watch::Sender<Option<TerminalLease>>,
}
impl Activity {
    pub fn new() -> (Self, watch::Receiver<Option<TerminalLease>>) {
        let (renewed, receiver) = watch::channel(None);
        (
            Self {
                input: AtomicBool::new(false),
                wake: Notify::new(),
                renewed,
            },
            receiver,
        )
    }
    pub fn record(&self) {
        self.input.store(true, Ordering::Release);
        self.wake.notify_one();
    }
}
pub(super) async fn renew(
    app: &Application,
    handle: &SessionGrantHandle,
    authority: &LeaseAuthority,
    activity: &Activity,
    mut events: Listener,
    computer: Uuid,
    family: RecordId,
) -> Result<(), ()> {
    let mut sequence = 0u64;
    loop {
        tokio::select! {
            biased;
            event = events.next(computer, &family) => event?,
            _ = activity.wake.notified() => {},
            _ = tokio::time::sleep(Duration::from_secs(5)) => {},
        }
        // Coalesce typing and event bursts. This wait is inside the independently
        // deadline-guarded renewal future, never an extension to existing authority.
        tokio::time::sleep(Duration::from_millis(500)).await;
        events.check()?;
        app.runtime.current().map_err(|_| ())?;
        let input = activity.input.swap(false, Ordering::AcqRel);
        let checked = app
            .store
            .renew_browser_grant(handle, input)
            .await
            .map_err(|_| ())?;
        events.check()?;
        authority
            .renew(
                checked.checked_at().into(),
                checked.valid_until() - checked.checked_at(),
            )
            .map_err(|_| ())?;
        sequence = sequence.checked_add(1).ok_or(())?;
        activity.renewed.send_replace(Some(TerminalLease {
            kind: TerminalLeaseKind::Lease,
            sequence,
            expires_at: (std::time::SystemTime::now()
                + checked
                    .valid_until()
                    .saturating_duration_since(std::time::Instant::now()))
            .into(),
        }));
    }
}
