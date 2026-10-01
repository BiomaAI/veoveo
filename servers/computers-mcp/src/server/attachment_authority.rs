use crate::Application;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::{Notify, watch};
use veoveo_computers::api::{TerminalLease, TerminalLeaseKind};
use veoveo_computers::session_grants::SessionGrantHandle;
use veoveo_computers::{AuthorityChanges, AuthorityInterest};
use veoveo_computers_runtime::LeaseAuthority;

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
pub(super) enum Grant<'a> {
    Browser(&'a SessionGrantHandle),
    Cli(&'a veoveo_computers::cli_grants::CliConnectionHandle),
}

pub(super) async fn renew(
    app: &Application,
    grant: Grant<'_>,
    authority: &LeaseAuthority,
    activity: &Activity,
    mut events: AuthorityChanges,
    interest: AuthorityInterest,
    mut valid_until: std::time::Instant,
) -> Result<(), ()> {
    let mut sequence = 0u64;
    loop {
        let renew_at = crate::io_guard::renew_at(valid_until)
            .min(tokio::time::Instant::now() + veoveo_computers_runtime::MAX_RENEWAL_INTERVAL);
        tokio::select! {
            biased;
            event = events.next(&interest) => event.map_err(|_| ())?,
            _ = activity.wake.notified() => {},
            _ = tokio::time::sleep_until(renew_at) => {},
        }
        // Coalesce typing and event bursts. This wait is inside the independently
        // deadline-guarded renewal future, never an extension to existing authority.
        tokio::time::sleep(Duration::from_millis(500)).await;
        events.check().map_err(|_| ())?;
        app.runtime.current().map_err(|_| ())?;
        let input = activity.input.swap(false, Ordering::AcqRel);
        let (checked_at, next_deadline) = match grant {
            Grant::Browser(handle) => {
                let checked = app
                    .store
                    .renew_browser_grant(handle, input)
                    .await
                    .map_err(|_| ())?;
                (checked.checked_at(), checked.valid_until())
            }
            Grant::Cli(handle) => {
                let checked = app
                    .store
                    .renew_cli_grant(handle, input)
                    .await
                    .map_err(|_| ())?;
                (checked.checked_at(), checked.valid_until())
            }
        };
        events.check().map_err(|_| ())?;
        authority
            .renew(checked_at.into(), next_deadline - checked_at)
            .map_err(|_| ())?;
        valid_until = next_deadline;
        sequence = sequence.checked_add(1).ok_or(())?;
        activity.renewed.send_replace(Some(TerminalLease {
            kind: TerminalLeaseKind::Lease,
            sequence,
            expires_at: (std::time::SystemTime::now()
                + valid_until.saturating_duration_since(std::time::Instant::now()))
            .into(),
        }));
    }
}
