//! I/O never conceals expiry while a current-authority read is pending.
use std::{
    future::Future,
    time::{Duration, Instant},
};
use veoveo_computers::commands::{CommandContinuation, CommandInterruption, CommandRunAuthority};

pub(super) enum Interrupted {
    Domain(CommandInterruption),
    LeaseLost,
}

pub(super) async fn run<T, F, R>(
    initial: CommandRunAuthority,
    pump: impl Future<Output = T>,
    check_every: Duration,
    mut refresh: impl FnMut() -> F,
    mut constrain_output: impl FnMut(u32) -> bool,
) -> Result<T, Interrupted>
where
    F: Future<Output = Result<CommandContinuation, R>>,
{
    tokio::pin!(pump);
    let mut authority = initial;
    let expires = tokio::time::sleep_until(authority.valid_until.into());
    let runtime = tokio::time::sleep_until(authority.execution_deadline.into());
    tokio::pin!(expires, runtime);
    let mut checks = tokio::time::interval(check_every);
    checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    checks.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = &mut expires => return Err(Interrupted::Domain(CommandInterruption::AuthorityLost)),
            _ = &mut runtime => return Err(Interrupted::Domain(authority.deadline_reason)),
            value = &mut pump => return Ok(value),
            checked = async { checks.tick().await; refresh().await } => {
                match checked {
                    Ok(CommandContinuation::Authorized(next)) => {
                        if next.valid_until <= Instant::now() { return Err(Interrupted::Domain(CommandInterruption::AuthorityLost)); }
                        if !constrain_output(next.maximum_output_bytes) { return Err(Interrupted::Domain(CommandInterruption::AuthorityLost)); }
                        if next.execution_deadline < authority.execution_deadline {
                            authority.execution_deadline = next.execution_deadline;
                            authority.deadline_reason = next.deadline_reason;
                            runtime.as_mut().reset(next.execution_deadline.into());
                        }
                        authority.valid_until = next.valid_until;
                        expires.as_mut().reset(next.valid_until.into());
                    }
                    Ok(CommandContinuation::Interrupted(reason)) => return Err(Interrupted::Domain(reason)),
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
