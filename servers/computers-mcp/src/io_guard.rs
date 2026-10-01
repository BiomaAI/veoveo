//! I/O never conceals expiry while a current-authority read is pending.
use futures::{Stream, StreamExt};
use std::{future::Future, time::Instant};
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
    mut changes: impl Stream<Item = veoveo_computers::Result<()>> + Unpin,
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
    let renew = tokio::time::sleep_until(renew_at(initial.valid_until()));
    tokio::pin!(renew);
    loop {
        tokio::select! {
            biased;
            _ = &mut expires => return Err(Interrupted::Domain(A::authority_lost())),
            _ = &mut runtime => return Err(Interrupted::Domain(deadline_reason)),
            checked = async {
                tokio::select! {
                    biased;
                    change = changes.next() => if !matches!(change, Some(Ok(()))) { return None; },
                    _ = &mut renew => {},
                }
                Some(refresh().await)
            } => {
                let Some(checked) = checked else { return Err(Interrupted::Domain(A::authority_lost())); };
                match checked.map(Into::into) {
                    Ok(Continuation::Authorized(next)) => {
                        if next.valid_until() <= Instant::now() || !constrain_output(next.maximum_bytes()) { return Err(Interrupted::Domain(A::authority_lost())); }
                        if next.deadline() < deadline {
                            deadline = next.deadline();
                            deadline_reason = next.deadline_reason();
                            runtime.as_mut().reset(deadline.into());
                        }
                        expires.as_mut().reset(next.valid_until().into());
                        renew.as_mut().reset(renew_at(next.valid_until()));
                    }
                    Ok(Continuation::Interrupted(reason)) => return Err(Interrupted::Domain(reason)),
                    Err(_) => return Err(Interrupted::LeaseLost),
                }
            }
            value = &mut pump => return Ok(value),
        }
    }
}

/// Reserve half the remaining permit for renewal while its expiry stays armed.
pub(crate) fn renew_at(deadline: Instant) -> tokio::time::Instant {
    let now = Instant::now();
    (now + deadline.saturating_duration_since(now) / 2).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{future::pending, time::Duration};
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
    async fn idle_guard_renews_from_expiry_and_changes_interrupt_before_renewal() {
        let reads = std::sync::atomic::AtomicUsize::new(0);
        let result = run(
            authority(100),
            tokio::time::sleep(Duration::from_millis(80)),
            futures::stream::pending(),
            || async {
                reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok::<_, ()>(CommandContinuation::Authorized(authority(10_000)))
            },
            |_| true,
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(reads.load(std::sync::atomic::Ordering::Relaxed), 1);

        let (sender, changes) = futures::channel::mpsc::unbounded();
        let send = async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            sender.unbounded_send(Ok(())).unwrap();
        };
        let guarded = run(
            authority(10_000),
            pending::<()>(),
            changes,
            || async {
                Ok::<_, ()>(CommandContinuation::Interrupted(
                    CommandInterruption::Cancelled,
                ))
            },
            |_| true,
        );
        let (_, result) = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(send, guarded)
        })
        .await
        .unwrap();
        assert!(matches!(
            result,
            Err(Interrupted::Domain(CommandInterruption::Cancelled))
        ));
    }

    #[tokio::test]
    async fn source_loss_closes_io_without_waiting_for_the_permit() {
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            run(
                authority(10_000),
                std::future::ready(()),
                futures::stream::empty(),
                pending::<Result<CommandContinuation, ()>>,
                |_| true,
            ),
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            Err(Interrupted::Domain(CommandInterruption::AuthorityLost))
        ));
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
                    futures::stream::iter([Ok(())]).chain(futures::stream::pending()),
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
                futures::stream::iter([Ok(())]).chain(futures::stream::pending()),
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
            futures::stream::iter([Ok(())]).chain(futures::stream::pending()),
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
