use crate::{Result, TransportError};
use chrono::{DateTime, Utc};
use std::time::Duration;
use tokio::{sync::watch, time::Instant};
use veoveo_computers_contract::{TERMINAL_VERSION, TerminalServerControl};

pub(crate) struct Deadline {
    state: watch::Sender<Instant>,
    pub ready: bool,
    replayed: bool,
    sequence: u64,
}
impl Deadline {
    pub fn new() -> (Self, watch::Receiver<Instant>) {
        let (state, receiver) = watch::channel(Instant::now() + Duration::from_secs(30));
        (
            Self {
                state,
                ready: false,
                replayed: false,
                sequence: 0,
            },
            receiver,
        )
    }
    pub fn control(&mut self, control: &TerminalServerControl) -> Result<()> {
        if *self.state.borrow() <= Instant::now() {
            return Err(TransportError::Expired);
        }
        let expires = match control {
            TerminalServerControl::Ready(value)
                if !self.ready && value.version == TERMINAL_VERSION =>
            {
                self.ready = true;
                Some(value.expires_at)
            }
            TerminalServerControl::Lease(value) if self.ready && value.sequence > self.sequence => {
                self.sequence = value.sequence;
                Some(value.expires_at)
            }
            TerminalServerControl::ReplayComplete(_) if self.ready && !self.replayed => {
                self.replayed = true;
                None
            }
            _ => return Err(TransportError::Protocol),
        };
        if let Some(expires) = expires {
            self.state
                .send_replace(window(expires, Utc::now(), Instant::now())?);
        }
        Ok(())
    }
}
fn window(expires: DateTime<Utc>, wall_now: DateTime<Utc>, monotonic: Instant) -> Result<Instant> {
    let remaining = (expires - wall_now)
        .to_std()
        .map_err(|_| TransportError::Expired)?;
    if remaining > Duration::from_secs(31) {
        return Err(TransportError::Protocol);
    }
    let bounded = remaining
        .checked_sub(Duration::from_secs(1))
        .filter(|d| !d.is_zero())
        .ok_or(TransportError::Expired)?;
    Ok(monotonic + bounded)
}
pub(crate) async fn expired(mut state: watch::Receiver<Instant>) {
    loop {
        let deadline = *state.borrow_and_update();
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => return,
            changed = state.changed() => { if changed.is_err() { return; } },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_computers_contract::*;
    fn ready(expires_at: DateTime<Utc>) -> TerminalServerControl {
        TerminalServerControl::Ready(TerminalReady {
            version: TERMINAL_VERSION,
            kind: TerminalReadyKind::Ready,
            expires_at,
        })
    }
    #[test]
    fn clock_allowance_is_charged_and_implausible_deadlines_fail() {
        let wall = Utc::now();
        let mono = Instant::now();
        assert_eq!(
            window(wall + chrono::TimeDelta::seconds(31), wall, mono).unwrap(),
            mono + Duration::from_secs(30)
        );
        for seconds in [-1, 0, 1, 32] {
            assert!(window(wall + chrono::TimeDelta::seconds(seconds), wall, mono).is_err());
        }
    }
    #[tokio::test]
    async fn duplicate_ready_stale_sequence_and_delayed_renewal_cannot_revive() {
        let (mut guard, receiver) = Deadline::new();
        let first = ready(Utc::now() + chrono::TimeDelta::seconds(3));
        guard.control(&first).unwrap();
        assert_eq!(guard.control(&first), Err(TransportError::Protocol));
        let update = TerminalServerControl::Lease(TerminalLease {
            kind: TerminalLeaseKind::Lease,
            sequence: 1,
            expires_at: Utc::now() + chrono::TimeDelta::seconds(3),
        });
        guard.control(&update).unwrap();
        assert_eq!(guard.control(&update), Err(TransportError::Protocol));
        expired(receiver).await;
        let fresh = TerminalServerControl::Lease(TerminalLease {
            kind: TerminalLeaseKind::Lease,
            sequence: 2,
            expires_at: Utc::now() + chrono::TimeDelta::seconds(30),
        });
        assert_eq!(guard.control(&fresh), Err(TransportError::Expired));
    }
}
