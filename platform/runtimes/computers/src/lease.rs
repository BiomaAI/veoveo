//! Local enforcement of authority checked by the owning Computers service.
use crate::{Result, RuntimeFailure};
use std::time::{Duration, SystemTime};
use tokio::{sync::watch, time::Instant};

pub const MAX_AUTHORITY_STALENESS: Duration = Duration::from_secs(30);
pub const MAX_RENEWAL_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
struct Window {
    checked_at: Instant,
    deadline: Instant,
}
impl Window {
    fn checked(checked_at: Instant, valid_for: Duration) -> Result<Self> {
        let now = Instant::now();
        if valid_for.is_zero() || valid_for > MAX_AUTHORITY_STALENESS || checked_at > now {
            return Err(RuntimeFailure::LeaseExpired);
        }
        let deadline = checked_at
            .checked_add(valid_for)
            .filter(|deadline| *deadline > now)
            .ok_or(RuntimeFailure::LeaseExpired)?;
        Ok(Self {
            checked_at,
            deadline,
        })
    }
}

/// Held only by the authority-checking task. Dropping it closes attachments.
///
/// Capture `checked_at` BEFORE the authoritative grant/policy read, and issue or
/// renew only after it succeeds. Read latency consumes the thirty-second bound.
/// Cached policy and an unavailable observer cannot authorize renewal. The owner
/// must also cap `valid_for` by the grant's remaining absolute/idle lifetime and
/// any installation clock allowance. This type never authorizes a principal.
pub struct LeaseAuthority {
    state: watch::Sender<Option<Window>>,
}

#[derive(Clone)]
pub struct AttachmentLease {
    state: watch::Receiver<Option<Window>>,
}

impl LeaseAuthority {
    pub fn issue(checked_at: Instant, valid_for: Duration) -> Result<(Self, AttachmentLease)> {
        let (state, receiver) = watch::channel(Some(Window::checked(checked_at, valid_for)?));
        Ok((Self { state }, AttachmentLease { state: receiver }))
    }

    pub fn renew(&self, checked_at: Instant, valid_for: Duration) -> Result<()> {
        let next = Window::checked(checked_at, valid_for)?;
        let mut accepted = false;
        self.state.send_if_modified(|state| {
            let Some(previous) = state else { return false };
            if previous.deadline <= Instant::now() {
                *state = None;
                return true;
            }
            // A delayed response cannot overwrite a newer authority check or
            // revive a revoked or expired connection. Tighter policy may shorten it.
            if next.checked_at <= previous.checked_at {
                return false;
            }
            *state = Some(next);
            accepted = true;
            true
        });
        if accepted {
            Ok(())
        } else {
            Err(RuntimeFailure::LeaseExpired)
        }
    }

    pub fn revoke(&self) {
        self.state.send_replace(None);
    }
}

impl AttachmentLease {
    pub(crate) async fn enforce<T>(&self, work: impl Future<Output = Result<T>>) -> Result<T> {
        self.check()?;
        tokio::select! {
            biased;
            _ = self.closed() => Err(RuntimeFailure::LeaseExpired),
            result = work => { self.check()?; result },
        }
    }

    pub fn check(&self) -> Result<()> {
        self.window().map(|_| ())
    }

    fn window(&self) -> Result<Window> {
        self.state
            .has_changed()
            .map_err(|_| RuntimeFailure::LeaseExpired)?;
        (*self.state.borrow())
            .filter(|window| window.deadline > Instant::now())
            .ok_or(RuntimeFailure::LeaseExpired)
    }

    /// Display projection only. Enforcement uses a monotonic deadline.
    pub fn expires_at(&self) -> Result<SystemTime> {
        Ok(SystemTime::now()
            + self
                .window()?
                .deadline
                .saturating_duration_since(Instant::now()))
    }

    /// Drives closure independently of socket reads, writes or output consumption.
    pub async fn closed(&self) {
        let mut state = self.state.clone();
        loop {
            let Ok(window) = self.window() else { return };
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(window.deadline) => {},
                changed = state.changed() => { if changed.is_err() { return; } },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn expiry_cannot_be_extended_by_delayed_checks_or_revocation_races() {
        let start = Instant::now();
        let (authority, lease) = LeaseAuthority::issue(start, Duration::from_secs(1)).unwrap();
        assert!(authority.renew(start, Duration::from_secs(2)).is_err());
        let newer = Instant::now();
        authority.renew(newer, Duration::from_secs(2)).unwrap();
        assert!(authority.renew(start, Duration::from_secs(3)).is_err());
        authority.revoke();
        assert!(
            authority
                .renew(Instant::now(), Duration::from_secs(3))
                .is_err()
        );
        assert!(lease.check().is_err());
        tokio::time::timeout(Duration::from_millis(50), lease.closed())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn dropped_authority_and_missed_renewals_close_without_io() {
        let (authority, lease) =
            LeaseAuthority::issue(Instant::now(), Duration::from_secs(30)).unwrap();
        drop(authority);
        assert!(lease.check().is_err());
        tokio::time::timeout(Duration::from_millis(50), lease.closed())
            .await
            .unwrap();
        let (authority, lease) =
            LeaseAuthority::issue(Instant::now(), Duration::from_millis(20)).unwrap();
        tokio::time::timeout(Duration::from_millis(100), lease.closed())
            .await
            .unwrap();
        assert!(
            authority
                .renew(Instant::now(), Duration::from_secs(30))
                .is_err()
        );
    }

    #[test]
    fn authority_latency_consumes_the_bound_and_future_or_overlong_windows_fail() {
        let now = Instant::now();
        assert!(LeaseAuthority::issue(now, Duration::from_secs(31)).is_err());
        assert!(LeaseAuthority::issue(now, Duration::ZERO).is_err());
        assert!(
            LeaseAuthority::issue(now + Duration::from_secs(1), Duration::from_secs(30)).is_err()
        );
        assert!(
            LeaseAuthority::issue(now - Duration::from_secs(31), Duration::from_secs(30)).is_err()
        );
        assert!(
            LeaseAuthority::issue(now - Duration::from_secs(29), Duration::from_secs(30)).is_ok()
        );
    }
}
