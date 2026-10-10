use super::*;
#[test]
fn retained_completion_rejects_changed_identity_independent_output_and_unsettled_receipt()
-> Result<()> {
    let requested = SubscriptionFilter::builder()
        .task_id("opaque-retained-schedule")
        .build();
    exact_ack(&requested, &requested)?;
    ensure!(exact_ack(&SubscriptionFilter::builder().build(), &requested).is_err());
    ensure!(
        exact_ack(
            &SubscriptionFilter::builder().task_id("foreign").build(),
            &requested
        )
        .is_err()
    );
    let original = super::super::tests::fixture()?;
    let id = CanonicalTaskId::parse("opaque-retained-schedule")?;
    let created = Task::new(
        id.to_string(),
        TaskStatus::Working,
        "2026-10-10T00:00:00Z",
        "2026-10-10T00:00:00Z",
    );
    let completed = Task::new(
        id.to_string(),
        TaskStatus::Completed,
        &created.created_at,
        "2026-10-10T00:00:01Z",
    );
    let mut journal = super::super::Journal::new(&original);
    journal.phase = Phase::Passed;
    journal.dispatch_intent = true;
    journal.task_id = Some(id.clone());
    journal.created = Some(TaskObservation::admit(&created)?);
    journal.current = Some(TaskObservation::admit(&completed)?);
    journal.delivered_completed = Some(TaskObservation::admit(&completed)?);
    journal.result = Some(original.expected.clone());
    journal.terminal_settled = true;
    journal.listener_closed = true;
    journal.caller_closed = true;
    let mut prior: Prior = serde_json::from_value(serde_json::to_value(journal)?)?;
    let created_at = created.created_at.parse()?;
    let admit = |prior: &Prior| {
        admit_prior(
            &original.installation,
            &original.installation.output,
            &id,
            created_at,
            &original,
            prior,
        )
    };
    admit(&prior)?;
    prior.current.as_mut().unwrap().task_id = CanonicalTaskId::parse("foreign-retained-task")?;
    ensure!(admit(&prior).is_err());
    prior.current.as_mut().unwrap().task_id = id.clone();
    prior.result.as_mut().unwrap().occurrences.clear();
    ensure!(admit(&prior).is_err());
    prior.result = Some(original.expected.clone());
    prior.listener_closed = false;
    ensure!(admit(&prior).is_err());
    prior.listener_closed = true;
    prior.mode = lifecycle::Mode::Recover;
    ensure!(admit(&prior).is_err());
    Ok(())
}

#[tokio::test]
async fn restoration_failure_still_closes_watches_with_original_reserved_deadline() -> Result<()> {
    use std::{cell::Cell, time::Instant};
    let end = Instant::now() + Duration::from_millis(100);
    // A short owner grace leaves no restoration allowance, but still owns close.
    ensure!(restoration_cutoff(end) < Instant::now());
    let closed = Cell::new(false);
    let outcomes = close_after_restoration(
        Err(anyhow::anyhow!("selected restoration failure")),
        end,
        |observed_end| {
            closed.set(true);
            async move {
                ensure!(
                    observed_end == end,
                    "cleanup extended its original deadline"
                );
                Ok(())
            }
        },
    )
    .await;
    ensure!(closed.get() && outcomes.restoration.is_err() && outcomes.watches.is_ok());
    let expired = Instant::now() - Duration::from_millis(1);
    closed.set(false);
    let late = close_after_restoration(Ok(()), expired, |observed_end| {
        closed.set(true);
        async move {
            ensure!(observed_end == expired);
            Ok(())
        }
    })
    .await;
    ensure!(closed.get() && late.watches.is_err());
    Ok(())
}
