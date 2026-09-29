use super::*;
use veoveo_stream_mcp::contract::{EncodedVideoChunk, LivePreviewView, LiveResultsView};
use veoveo_stream_mcp::{
    contract::{PipelineId, SessionId, SessionPreviewUri, SessionResultsUri},
    uris as stream_uris,
};

pub(crate) async fn verify(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    installation.operator.validate_credentials()?;
    let operator = OperatorClient {
        conformance,
        installation,
    };
    let state: veoveo_uav_sim_mcp::contract::SimulationState =
        serde_json::from_value(simulation_state(&operator, &scenario).await?)
            .context("decoding the UAV-owned camera state")?;
    ensure!(
        state.session_id == scenario.session_id
            && state.cameras.iter().any(|camera| {
                camera.vehicle_id == scenario.vehicle_id
                    && camera.lifecycle == veoveo_uav_sim_mcp::contract::CameraLifecycle::Ready
                    && camera.frames_observed >= 3
                    && camera.last_access_unit_bytes > 0
            }),
        "the selected UAV camera must publish NVIDIA NVENC access units before Stream acceptance"
    );
    let live = prepare_live_stream_pipeline(&operator, &scenario.stream.live_pipeline_id).await?;
    let result = wait_for_live_stream(
        &operator,
        &live.session_id,
        &live.preview_uri,
        &scenario.stream,
    )
    .await;
    if let Err(error) = &result {
        eprintln!("live Stream acceptance failed; starting owned cleanup: {error:#}");
    }
    let cleanup = if live.owned_by_acceptance {
        stop_live_stream_session(&operator, &live.session_id, "Stream acceptance cleanup").await
    } else {
        Ok(())
    };
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => bail!("{error:#}; Stream cleanup also failed: {cleanup:#}"),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => return Err(error),
        (Ok(()), Ok(())) => {}
    }
    println!(
        "UAV live Stream acceptance passed: fresh inference and encoded preview; session {}",
        live.session_id
    );
    Ok(())
}

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
                session.session_id(),
                pipeline_id
            );
            return acceptance_live_session(
                &session.session_id(),
                &session.results_uri(),
                &session.preview_uri(),
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
    let session: LiveSessionView = serde_json::from_value(
        operator
            .resource(&started.result_uri().to_string(), Duration::from_secs(60))
            .await?,
    )
    .context("reading the started Stream session result")?;
    ensure!(
        session.session_id() == started.session_id()
            && session.pipeline_id() == started.pipeline_id(),
        "Stream start result resource does not match its tool output"
    );
    acceptance_live_session(
        &started.session_id(),
        &started.results_uri(),
        &started.preview_uri(),
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
            session.pipeline_id() == pipeline_id
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
        output.result_uri.id() == session_id,
        "Stream stop returned another session"
    );
    let session: LiveSessionView = serde_json::from_value(
        operator
            .resource(&output.result_uri.to_string(), Duration::from_secs(60))
            .await?,
    )
    .context("reading the stopped Stream session result")?;
    ensure!(
        session.session_id() == *session_id && session.lifecycle == LiveSessionLifecycle::Stopped,
        "Stream stop result resource does not describe the stopped session"
    );
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
) -> Result<()> {
    let session_uri = stream_uris::session_uri(*session_id).to_string();
    let results_uri = stream_uris::session_results_uri(*session_id).to_string();
    let timeout = Duration::from_secs(acceptance.live_timeout_seconds);
    tokio::time::timeout(timeout, async {
        loop {
            let session: LiveSessionView = serde_json::from_value(
                operator
                    .resource(&session_uri, Duration::from_secs(60))
                    .await?,
            )
            .context("decoding live Stream session")?;
            ensure!(
                session.session_id() == *session_id
                    && session.pipeline_id() == &acceptance.live_pipeline_id,
                "Stream session does not match the admitted session and pipeline"
            );
            ensure!(
                matches!(
                    session.lifecycle,
                    LiveSessionLifecycle::Starting | LiveSessionLifecycle::Running
                ),
                "live Stream session is {:?}: {}",
                session.lifecycle,
                session.error.as_deref().unwrap_or("no diagnostic")
            );
            let results: LiveResultsView = serde_json::from_value(
                operator
                    .resource(&results_uri, Duration::from_secs(60))
                    .await?,
            )
            .context("decoding live Stream results")?;
            let preview: LivePreviewView = serde_json::from_value(
                operator
                    .resource(&preview_uri.to_string(), Duration::from_secs(60))
                    .await?,
            )
            .context("decoding live Stream preview")?;
            ensure!(
                results.session_id == *session_id
                    && preview.session_id == *session_id
                    && results.pipeline_id == acceptance.live_pipeline_id,
                "Stream results or preview belong to another session or pipeline"
            );

            let enough_frames = results.processed_frames >= acceptance.minimum_live_frames;
            let fresh = results
                .frames
                .last()
                .and_then(|frame| DateTime::parse_from_rfc3339(&frame.observed_at).ok())
                .is_some_and(|observed_at| {
                    let age = Utc::now()
                        .signed_duration_since(observed_at.with_timezone(&Utc))
                        .num_milliseconds();
                    age >= 0
                        && age
                            <= i64::try_from(acceptance.maximum_result_age_ms).unwrap_or(i64::MAX)
                });
            let decodable_preview = validate_live_preview(&preview.chunks).is_ok();
            if enough_frames && fresh && decodable_preview {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .with_context(|| {
        format!(
            "Stream produced no fresh typed results and decodable App preview within {timeout:?}"
        )
    })?
}

pub(super) fn validate_live_preview(chunks: &[EncodedVideoChunk]) -> Result<()> {
    ensure!(
        chunks.first().is_some_and(|chunk| chunk.keyframe),
        "live preview must begin at a keyframe"
    );
    let mut last_sequence: Option<u64> = None;
    let mut timestamps = BTreeSet::new();
    for chunk in chunks {
        let sequence = chunk.sequence;
        let timestamp = chunk.timestamp_us;
        if let Some(previous) = last_sequence {
            ensure!(
                Some(sequence) == previous.checked_add(1),
                "live preview sequence is not contiguous"
            );
        }
        ensure!(
            timestamps.insert(timestamp),
            "live preview repeated a presentation timestamp"
        );
        let bytes = BASE64_STANDARD
            .decode(&chunk.data_base64)
            .context("live preview chunk is not valid base64")?;
        ensure!(
            bytes.starts_with(&[0, 0, 0, 1]) || bytes.starts_with(&[0, 0, 1]),
            "live preview chunk is not Annex B H.264"
        );
        last_sequence = Some(sequence);
    }
    Ok(())
}
