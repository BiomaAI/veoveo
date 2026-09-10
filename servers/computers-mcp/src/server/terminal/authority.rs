use super::super::access_events::Listener;
use crate::Application;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::Notify;
use uuid::Uuid;
use veoveo_computers::session_grants::SessionGrantHandle;
use veoveo_computers_runtime::LeaseAuthority;
use veoveo_platform_store::RecordId;

#[derive(Default)]
pub(super) struct Activity {
    input: AtomicBool,
    wake: Notify,
}
impl Activity {
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
    }
}
