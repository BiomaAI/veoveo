//! I/O never conceals expiry while a current-authority read is pending.
use std::{
    future::Future,
    time::{Duration, Instant},
};
use veoveo_computers::{
    commands::{CommandContinuation, CommandInterruption, CommandRunAuthority},
    files::{FileContinuation, FileInterruption, FileRunAuthority},
};

pub(crate) trait Permit {
    type Reason: Copy;
    fn valid_until(&self) -> Instant;
    fn deadline(&self) -> Instant;
    fn maximum_bytes(&self) -> u64;
    fn deadline_reason(&self) -> Self::Reason;
    fn authority_lost() -> Self::Reason;
}
pub(crate) enum Continuation<A: Permit> {
    Authorized(A),
    Interrupted(A::Reason),
}
pub(crate) enum Interrupted<R> {
    Domain(R),
    LeaseLost,
}

impl Permit for CommandRunAuthority {
    type Reason = CommandInterruption;
    fn valid_until(&self) -> Instant {
        self.valid_until
    }
    fn deadline(&self) -> Instant {
        self.execution_deadline
    }
    fn maximum_bytes(&self) -> u64 {
        u64::from(self.maximum_output_bytes)
    }
    fn deadline_reason(&self) -> Self::Reason {
        self.deadline_reason
    }
    fn authority_lost() -> Self::Reason {
        CommandInterruption::AuthorityLost
    }
}
impl From<CommandContinuation> for Continuation<CommandRunAuthority> {
    fn from(value: CommandContinuation) -> Self {
        match value {
            CommandContinuation::Authorized(a) => Self::Authorized(a),
            CommandContinuation::Interrupted(r) => Self::Interrupted(r),
        }
    }
}
impl Permit for FileRunAuthority {
    type Reason = FileInterruption;
    fn valid_until(&self) -> Instant {
        self.valid_until
    }
    fn deadline(&self) -> Instant {
        self.execution_deadline
    }
    fn maximum_bytes(&self) -> u64 {
        self.maximum_bytes
    }
    fn deadline_reason(&self) -> Self::Reason {
        self.deadline_reason
    }
    fn authority_lost() -> Self::Reason {
        FileInterruption::AuthorityLost
    }
}
impl From<FileContinuation> for Continuation<FileRunAuthority> {
    fn from(value: FileContinuation) -> Self {
        match value {
            FileContinuation::Authorized(a) => Self::Authorized(a),
            FileContinuation::Interrupted(r) => Self::Interrupted(r),
        }
    }
}

pub(crate) async fn run<T, F, R, A: Permit, C: Into<Continuation<A>>>(
    initial: A,
    pump: impl Future<Output = T>,
    check_every: Duration,
    mut refresh: impl FnMut() -> F,
    mut constrain_output: impl FnMut(u64) -> bool,
) -> Result<T, Interrupted<A::Reason>>
where
    F: Future<Output = Result<C, R>>,
{
    tokio::pin!(pump);
    let mut deadline = initial.deadline();
    let mut deadline_reason = initial.deadline_reason();
    let expires = tokio::time::sleep_until(initial.valid_until().into());
    let runtime = tokio::time::sleep_until(deadline.into());
    tokio::pin!(expires, runtime);
    let mut checks = tokio::time::interval(check_every);
    checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    checks.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = &mut expires => return Err(Interrupted::Domain(A::authority_lost())),
            _ = &mut runtime => return Err(Interrupted::Domain(deadline_reason)),
            value = &mut pump => return Ok(value),
            checked = async { checks.tick().await; refresh().await } => {
                match checked.map(Into::into) {
                    Ok(Continuation::Authorized(next)) => {
                        if next.valid_until() <= Instant::now() || !constrain_output(next.maximum_bytes()) { return Err(Interrupted::Domain(A::authority_lost())); }
                        if next.deadline() < deadline {
                            deadline = next.deadline();
                            deadline_reason = next.deadline_reason();
                            runtime.as_mut().reset(deadline.into());
                        }
                        expires.as_mut().reset(next.valid_until().into());
                    }
                    Ok(Continuation::Interrupted(reason)) => return Err(Interrupted::Domain(reason)),
                    Err(_) => return Err(Interrupted::LeaseLost),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::pending;
    fn authority(milliseconds: u64) -> CommandRunAuthority {
        let now = Instant::now();
        CommandRunAuthority {
            valid_until: now + Duration::from_millis(milliseconds),
            execution_deadline: now + Duration::from_secs(10),
            maximum_output_bytes: 1024,
            deadline_reason: CommandInterruption::Deadline,
        }
    }
    #[tokio::test]
    async fn a_blocked_refresh_cannot_hide_authority_expiry_or_execution_deadline() {
        for runtime in [false, true] {
            let mut initial = authority(if runtime { 10000 } else { 40 });
            if runtime {
                initial.execution_deadline = Instant::now() + Duration::from_millis(40);
            }
            let result = tokio::time::timeout(
                Duration::from_secs(1),
                run(
                    initial,
                    pending::<()>(),
                    Duration::from_millis(1),
                    pending::<Result<CommandContinuation, ()>>,
                    |_| true,
                ),
            )
            .await
            .unwrap();
            assert!(
                matches!(result, Err(Interrupted::Domain(reason)) if reason == if runtime {CommandInterruption::Deadline} else {CommandInterruption::AuthorityLost})
            );
        }
    }
    #[tokio::test]
    async fn policy_updates_can_only_shorten_runtime_and_cannot_exceed_output_already_observed() {
        let mut initial = authority(1000);
        initial.execution_deadline = Instant::now() + Duration::from_millis(40);
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            run(
                initial,
                pending::<()>(),
                Duration::from_millis(1),
                || async { Ok::<_, ()>(CommandContinuation::Authorized(authority(1000))) },
                |_| true,
            ),
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            Err(Interrupted::Domain(CommandInterruption::Deadline))
        ));
        let result = run(
            authority(1000),
            pending::<()>(),
            Duration::from_millis(1),
            || async {
                let mut reduced = authority(1000);
                reduced.maximum_output_bytes = 4;
                Ok::<_, ()>(CommandContinuation::Authorized(reduced))
            },
            |maximum| maximum >= 5,
        )
        .await;
        assert!(matches!(
            result,
            Err(Interrupted::Domain(CommandInterruption::AuthorityLost))
        ));
    }
}
