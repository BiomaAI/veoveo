use super::*;
#[test]
fn transition_rejects_wrong_identity_due_and_version() -> Result<()> {
    let binding = veoveo_time_mcp::AuthorityBinding::new(
        "time-release-tzdb".parse().map_err(anyhow::Error::msg)?,
        "time-release-leaps".parse().map_err(anyhow::Error::msg)?,
    )?;
    let created = TemporalEvent {
        event_id: "event-fixture".parse().map_err(anyhow::Error::msg)?,
        name: "fixture".into(),
        due: TimeInstant::from_total_nanoseconds(1, 0, binding)?,
        state: TemporalEventState::Scheduled,
        record_version: veoveo_time_mcp::TimeVersion::FIRST,
    };
    let mut current = created.clone();
    current.state = TemporalEventState::Due;
    current.record_version = current.record_version.checked_next()?;
    unchanged(&created, &current, TemporalEventState::Due)?;
    let mut wrong = current.clone();
    wrong.due.tai_seconds_since_1970 += 1;
    ensure!(unchanged(&created, &wrong, TemporalEventState::Due).is_err());
    let mut wrong = current.clone();
    wrong.event_id = "event-other".parse().map_err(anyhow::Error::msg)?;
    ensure!(unchanged(&created, &wrong, TemporalEventState::Due).is_err());
    let mut wrong = current;
    wrong.record_version = created.record_version;
    ensure!(unchanged(&created, &wrong, TemporalEventState::Due).is_err());
    Ok(())
}

#[tokio::test]
async fn unknown_create_cannot_settle_ownership_after_local_handles_close() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let binding = veoveo_time_mcp::AuthorityBinding::new(
        "time-release-tzdb".parse().map_err(anyhow::Error::msg)?,
        "time-release-leaps".parse().map_err(anyhow::Error::msg)?,
    )?;
    let request = CreateTemporalEventRequest {
        name: "unknown native intent".into(),
        due: TimeInstant::from_total_nanoseconds(1, 0, binding)?,
        idempotency_key: uuid::Uuid::now_v7().to_string(),
    };
    let j = Arc::new(Mutex::new(Journal {
        file: open_receipt(&directory.path().join("receipt.jsonl"))?,
        evidence: Evidence {
            schema: "veoveo.ai/time-event-acceptance/v1",
            phase: Phase::Created,
            intents: vec![Intent {
                request,
                acknowledged: None,
            }],
            cancellation_intents: vec![],
            creation_replay_intents: vec![],
            observations: vec![],
            delivered_invalidations: 0,
            selected: None,
            restart_intent: false,
            restart: None,
            disconnected_due: None,
            listener_closed: true,
            caller_closed: true,
            failed: false,
        },
    }));
    journal(&j, |j| j.persist())?;
    let handles = Arc::new(AsyncMutex::new(Handles::default()));
    let retained = handles.clone();
    let evidence = j.clone();
    let result: Result<()> = owner::run(async {
        owner::register_cleanup(
            owner::CleanupKind::Remote,
            "native unknown Time event intent",
            &uuid::Uuid::now_v7().to_string(),
            move || async move { cleanup(&mut *retained.lock().await, &evidence).await },
        )?;
        Ok(())
    })
    .await;
    ensure!(
        result.is_err(),
        "unknown create incorrectly settled remote ownership"
    );
    let handles = handles.lock().await;
    ensure!(
        handles.generations.is_empty(),
        "unknown intent caused a new connection or replay"
    );
    let journal = j
        .lock()
        .map_err(|_| anyhow::anyhow!("native journal poisoned"))?;
    ensure!(
        journal.evidence.intents.len() == 1 && journal.evidence.intents[0].acknowledged.is_none(),
        "unknown intent was discarded"
    );
    Ok(())
}

#[test]
fn cleanup_wrong_scheduled_identity_refuses_before_cancel_dispatch() -> Result<()> {
    let binding = veoveo_time_mcp::AuthorityBinding::new(
        "time-release-tzdb".parse().map_err(anyhow::Error::msg)?,
        "time-release-leaps".parse().map_err(anyhow::Error::msg)?,
    )?;
    let acknowledged = TemporalEvent {
        event_id: "event-owned".parse().map_err(anyhow::Error::msg)?,
        name: "owned fixture".into(),
        due: TimeInstant::from_total_nanoseconds(1, 0, binding)?,
        state: TemporalEventState::Scheduled,
        record_version: veoveo_time_mcp::TimeVersion::FIRST,
    };
    let mut wrong = acknowledged.clone();
    wrong.event_id = "event-foreign".parse().map_err(anyhow::Error::msg)?;
    let mut dispatches = Vec::new();
    let result = cleanup_cancellation(&acknowledged, &wrong).map(|request| {
        if let Some(request) = request {
            dispatches.push(request);
        }
    });
    ensure!(
        result.is_err() && dispatches.is_empty(),
        "wrong event reached cancellation dispatch"
    );
    let request = cleanup_cancellation(&acknowledged, &acknowledged)?
        .context("owned Scheduled cancellation")?;
    ensure!(request.event_id == acknowledged.event_id);
    ensure!(request.expected_record_version == acknowledged.record_version);
    let mut due = acknowledged.clone();
    due.state = TemporalEventState::Due;
    ensure!(cleanup_cancellation(&acknowledged, &due)?.is_none());
    Ok(())
}
