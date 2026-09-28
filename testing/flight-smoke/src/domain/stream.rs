use super::*;
use veoveo_stream_mcp::{
    contract::{PipelineId, SessionId, SessionPreviewUri, SessionResultsUri},
    uris as stream_uris,
};

pub(super) struct AcceptanceLiveSession {
    pub(super) session_id: SessionId,
    pub(super) preview_uri: SessionPreviewUri,
    pub(super) owned_by_acceptance: bool,
}

pub(super) async fn prepare_live_stream_pipeline(
    operator: &OperatorClient<'_>,
    pipeline_id: &PipelineId,
) -> Result<AcceptanceLiveSession> {
    // Bound the preflight over domain-owned pages. Exhaustion fails before starting
    // a duplicate runner when the active session lies beyond our read budget.
    let mut uri = "stream://sessions".to_owned();
    let mut seen = BTreeSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    for page_number in 0..100 {
        let page: veoveo_stream_mcp::contract::LiveSessionsPage = serde_json::from_value(
            operator
                .resource(
                    &uri,
                    deadline.saturating_duration_since(tokio::time::Instant::now()),
                )
                .await?,
        )
        .context("decoding visible live Stream session page")?;
        ensure!(
            page.limit == 100 && page.sessions.len() <= page.limit,
            "invalid Stream session page bound"
        );
        if let Some(session) = reusable_live_stream_session(&page.sessions, pipeline_id)? {
            eprintln!(
                "preflight: reusing visible {} live Stream session {} for pipeline {} without taking ownership",
                match session.lifecycle {
                    LiveSessionLifecycle::Starting => "starting",
                    LiveSessionLifecycle::Running => "running",
                    LiveSessionLifecycle::Failed | LiveSessionLifecycle::Stopped => unreachable!(),
                },
                session.session_id,
                pipeline_id
            );
            return acceptance_live_session(
                &session.session_id,
                &session.results_uri,
                &session.preview_uri,
                false,
            );
        }
        let Some(cursor) = page.next_cursor else {
            break;
        };
        ensure!(
            page_number < 99,
            "Stream session preflight exceeded 100 pages"
        );
        ensure!(
            seen.insert(cursor.as_str().to_owned()),
            "Stream session cursor repeated"
        );
        uri = stream_uris::sessions_uri(Some(cursor)).to_string();
    }

    let started: StartLiveSessionOutput = serde_json::from_value(
        operator
            .call_tool(
                "stream__start_live_session",
                serde_json::json!({"pipeline_id": pipeline_id}),
            )
            .await
            .context("starting the recording-independent live Stream session")?,
    )
    .context("decoding the typed live Stream session")?;
    acceptance_live_session(
        &started.session_id,
        &started.results_uri,
        &started.preview_uri,
        true,
    )
}

pub(super) fn reusable_live_stream_session<'a>(
    sessions: &'a [LiveSessionView],
    pipeline_id: &PipelineId,
) -> Result<Option<&'a LiveSessionView>> {
    let active = sessions
        .iter()
        .filter(|session| {
            &session.pipeline_id == pipeline_id
                && matches!(
                    session.lifecycle,
                    LiveSessionLifecycle::Starting | LiveSessionLifecycle::Running
                )
        })
        .collect::<Vec<_>>();
    ensure!(
        active.len() <= 1,
        "Stream exposed multiple active sessions for admitted pipeline {pipeline_id}: {active:?}"
    );
    Ok(active.into_iter().next())
}

pub(super) fn acceptance_live_session(
    session_id: &SessionId,
    results_uri: &SessionResultsUri,
    preview_uri: &SessionPreviewUri,
    owned_by_acceptance: bool,
) -> Result<AcceptanceLiveSession> {
    ensure!(
        results_uri.id() == session_id && preview_uri.id() == session_id,
        "Stream returned inconsistent live-session resources for {session_id}: results={results_uri}, preview={preview_uri}"
    );
    Ok(AcceptanceLiveSession {
        session_id: *session_id,
        preview_uri: preview_uri.to_owned(),
        owned_by_acceptance,
    })
}

pub(super) async fn stop_live_stream_session(
    operator: &OperatorClient<'_>,
    session_id: &SessionId,
    phase: &str,
) -> Result<()> {
    let output: StopLiveSessionOutput = serde_json::from_value(
        operator
            .call_tool(
                "stream__stop_live_session",
                serde_json::json!({"session_id": session_id}),
            )
            .await
            .with_context(|| format!("{phase}: stopping live Stream session {session_id}"))?,
    )
    .with_context(|| format!("{phase}: decoding stopped live Stream session {session_id}"))?;
    ensure!(
        output.lifecycle == LiveSessionLifecycle::Stopped,
        "{phase}: live Stream session {session_id} did not stop cleanly: {output:?}"
    );
    Ok(())
}

pub(super) async fn wait_for_live_stream(
    operator: &OperatorClient<'_>,
    session_id: &SessionId,
    preview_uri: &SessionPreviewUri,
    acceptance: &StreamScenario,
) -> Result<Value> {
    let session_uri = stream_uris::session_uri(*session_id).to_string();
    let results_uri = stream_uris::session_results_uri(*session_id).to_string();
    let timeout = Duration::from_secs(acceptance.live_timeout_seconds);
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let session = operator
            .resource(&session_uri, Duration::from_secs(60))
            .await?;
        ensure!(
            json_string(&session, "/lifecycle")? != "failed",
            "live Stream session failed: {session}"
        );
        let results = operator
            .resource(&results_uri, Duration::from_secs(60))
            .await?;
        let preview = operator
            .resource(&preview_uri.to_string(), Duration::from_secs(60))
            .await?;
        let current = serde_json::json!({
            "session": session,
            "results": results,
            "preview": preview
        });

        let enough_frames = current
            .pointer("/results/processed_frames")
            .and_then(Value::as_u64)
            .is_some_and(|count| count >= acceptance.minimum_live_frames);
        let latest_frame = current
            .pointer("/results/frames")
            .and_then(Value::as_array)
            .and_then(|frames| frames.last());
        let fresh = latest_frame
            .and_then(|frame| frame.get("observed_at"))
            .and_then(Value::as_str)
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_some_and(|observed_at| {
                let age = Utc::now()
                    .signed_duration_since(observed_at.with_timezone(&Utc))
                    .num_milliseconds();
                age >= 0
                    && age <= i64::try_from(acceptance.maximum_result_age_ms).unwrap_or(i64::MAX)
            });
        let chunks = current
            .pointer("/preview/chunks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let decodable_preview = validate_live_preview(&chunks).is_ok();
        if enough_frames && fresh && decodable_preview {
            validate_live_preview(&chunks)?;
            return Ok(current);
        }
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "Stream produced no fresh typed results and decodable App preview within \
                 {timeout:?}: {current}"
            );
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

pub(super) fn validate_live_preview(chunks: &[Value]) -> Result<()> {
    ensure!(
        chunks.first().and_then(|chunk| chunk.get("keyframe")) == Some(&Value::Bool(true)),
        "live preview must begin at a keyframe"
    );
    let mut last_sequence = None;
    let mut timestamps = BTreeSet::new();
    for chunk in chunks {
        let sequence = chunk
            .get("sequence")
            .and_then(Value::as_u64)
            .context("live preview chunk omitted sequence")?;
        let timestamp = chunk
            .get("timestamp_us")
            .and_then(Value::as_u64)
            .context("live preview chunk omitted timestamp_us")?;
        if let Some(previous) = last_sequence {
            ensure!(
                sequence == previous + 1,
                "live preview sequence is not contiguous"
            );
        }
        ensure!(
            timestamps.insert(timestamp),
            "live preview repeated a presentation timestamp"
        );
        let encoded = chunk
            .get("data_base64")
            .and_then(Value::as_str)
            .context("live preview chunk omitted data_base64")?;
        let bytes = BASE64_STANDARD
            .decode(encoded)
            .context("live preview chunk is not valid base64")?;
        ensure!(
            bytes.starts_with(&[0, 0, 0, 1]) || bytes.starts_with(&[0, 0, 1]),
            "live preview chunk is not Annex B H.264"
        );
        last_sequence = Some(sequence);
    }
    Ok(())
}
