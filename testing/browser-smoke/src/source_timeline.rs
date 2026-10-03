//! Align browser observations with the running simulator's published clock.

use std::{future::Future, time::Duration};

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_uav_sim_mcp::contract::SimulationLifecycle;

pub(crate) const SOURCE_SAMPLE_TIMEOUT: Duration = Duration::from_secs(10);
const SOURCE_SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceTimelineSample {
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) simulation_time_seconds: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceTimelineWindowEvidence {
    pub(crate) before: SourceTimelineSample,
    pub(crate) after: SourceTimelineSample,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceTimelineAlignmentEvidence {
    before: SourceTimelineSample,
    after: SourceTimelineSample,
    recording_observed_at: DateTime<Utc>,
    interpolation_fraction: f64,
    pub(crate) aligned_simulation_time_seconds: f64,
}

pub(crate) fn source_timeline_sample(state: &Value) -> Result<SourceTimelineSample> {
    #[derive(Deserialize)]
    struct SourceState {
        lifecycle: SimulationLifecycle,
        updated_at: DateTime<Utc>,
        simulation_time_s: f64,
    }

    let state: SourceState =
        serde_json::from_value(state.clone()).context("decoding simulation source timeline")?;
    ensure!(
        state.lifecycle == SimulationLifecycle::Running,
        "source timeline requires a running simulation, received {:?}",
        state.lifecycle
    );
    ensure!(
        state.simulation_time_s.is_finite() && state.simulation_time_s >= 0.0,
        "running simulation returned invalid simulation_time_s {}",
        state.simulation_time_s
    );
    Ok(SourceTimelineSample {
        updated_at: state.updated_at,
        simulation_time_seconds: state.simulation_time_s,
    })
}

#[allow(dead_code)] // Used by focused live-camera acceptance, not composed flight.
pub(crate) fn source_timeline_window(
    before: &Value,
    after: &Value,
) -> Result<SourceTimelineWindowEvidence> {
    timeline_window(
        source_timeline_sample(before)?,
        source_timeline_sample(after)?,
    )
}

fn timeline_window(
    before: SourceTimelineSample,
    after: SourceTimelineSample,
) -> Result<SourceTimelineWindowEvidence> {
    ensure!(
        after.updated_at > before.updated_at
            && after.simulation_time_seconds > before.simulation_time_seconds,
        "running simulation source timeline did not advance: {before:?} -> {after:?}"
    );
    Ok(SourceTimelineWindowEvidence { before, after })
}

fn align_source_timeline(
    before: SourceTimelineSample,
    after: SourceTimelineSample,
    recording_observed_at: DateTime<Utc>,
) -> Result<SourceTimelineAlignmentEvidence> {
    timeline_window(before, after)?;
    ensure!(
        (before.updated_at..=after.updated_at).contains(&recording_observed_at),
        "Rerun observation is not bracketed by source samples: before={before:?} observation={recording_observed_at} after={after:?}"
    );
    let total_nanoseconds = (after.updated_at - before.updated_at)
        .num_nanoseconds()
        .context("source timeline bracket exceeds supported duration")?;
    let observed_nanoseconds = (recording_observed_at - before.updated_at)
        .num_nanoseconds()
        .context("source timeline observation exceeds supported duration")?;
    let interpolation_fraction = observed_nanoseconds as f64 / total_nanoseconds as f64;
    let aligned_simulation_time_seconds = before.simulation_time_seconds
        + (after.simulation_time_seconds - before.simulation_time_seconds) * interpolation_fraction;
    Ok(SourceTimelineAlignmentEvidence {
        before,
        after,
        recording_observed_at,
        interpolation_fraction,
        aligned_simulation_time_seconds,
    })
}

/// A read made after capture can still return a sample published before capture.
/// Retain the closest earlier sample and wait for a later one; never extrapolate.
/// One deadline covers source reads and the waits between them.
pub(crate) async fn sample_source_alignment<F, Fut>(
    initial: SourceTimelineSample,
    observed_at: DateTime<Utc>,
    timeout: Duration,
    mut read: F,
) -> Result<SourceTimelineAlignmentEvidence>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<SourceTimelineSample>>,
{
    ensure!(
        initial.updated_at <= observed_at,
        "initial source sample follows the browser observation: {initial:?}, observation={observed_at}"
    );
    let mut before = initial;
    tokio::time::timeout(timeout, async {
        loop {
            let after = read().await?;
            ensure!(
                after.updated_at >= before.updated_at
                    && after.simulation_time_seconds >= before.simulation_time_seconds
                    && (after.updated_at != before.updated_at
                        || after.simulation_time_seconds == before.simulation_time_seconds),
                "source timeline moved backwards or changed at the same timestamp: {before:?} -> {after:?}"
            );
            if after.updated_at >= observed_at && after.updated_at > before.updated_at {
                return align_source_timeline(before, after, observed_at);
            }
            before = after;
            tokio::time::sleep(SOURCE_SAMPLE_INTERVAL).await;
        }
    })
    .await
    .with_context(|| {
        format!(
            "source timeline did not bracket browser observation {observed_at} within {timeout:?}; last sample={before:?}"
        )
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(timestamp: &str, seconds: f64) -> SourceTimelineSample {
        source_timeline_sample(&serde_json::json!({
            "lifecycle": "running",
            "updated_at": timestamp,
            "simulation_time_s": seconds
        }))
        .unwrap()
    }

    #[test]
    fn alignment_interpolates_and_rejects_extrapolation_or_no_progress() {
        let before = sample("2026-08-05T12:00:00Z", 100.0);
        let after = sample("2026-08-05T12:00:20Z", 120.0);
        let observed = before.updated_at + chrono::Duration::milliseconds(7500);
        let aligned = align_source_timeline(before, after, observed).unwrap();
        assert_eq!(aligned.interpolation_fraction, 0.375);
        assert_eq!(aligned.aligned_simulation_time_seconds, 107.5);
        for outside in [
            before.updated_at - chrono::Duration::seconds(1),
            after.updated_at + chrono::Duration::seconds(1),
        ] {
            assert!(align_source_timeline(before, after, outside).is_err());
        }
        assert!(align_source_timeline(before, before, observed).is_err());
        assert!(align_source_timeline(after, before, observed).is_err());
    }

    #[tokio::test]
    async fn source_sample_published_before_capture_waits_for_a_real_bracket() {
        let before = sample("2026-10-03T21:47:35.602470Z", 455.3333333333333);
        let stale = sample("2026-10-03T21:50:00.581627Z", 600.3666666666667);
        let fresh = sample("2026-10-03T21:50:00.781627Z", 600.5666666666667);
        let observed = DateTime::parse_from_rfc3339("2026-10-03T21:50:00.686611331Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(align_source_timeline(before, stale, observed).is_err());
        let mut samples = [stale, stale, fresh].into_iter();
        let aligned = sample_source_alignment(before, observed, SOURCE_SAMPLE_TIMEOUT, || {
            std::future::ready(Ok(samples.next().expect("unexpected extra read")))
        })
        .await
        .unwrap();
        assert_eq!(aligned.before.updated_at, stale.updated_at);
        assert_eq!(aligned.after.updated_at, fresh.updated_at);
        assert!((aligned.aligned_simulation_time_seconds - 600.4716509976667).abs() < 1e-9);
    }

    #[tokio::test]
    async fn sampler_rejects_regression_and_propagates_source_errors() {
        let initial = sample("2026-08-05T12:00:00Z", 100.0);
        let observed = initial.updated_at + chrono::Duration::seconds(2);
        for invalid in [
            sample("2026-08-05T11:59:59Z", 101.0),
            sample("2026-08-05T12:00:01Z", 99.0),
            sample("2026-08-05T12:00:00Z", 101.0),
            sample("2026-08-05T12:00:02Z", 100.0),
        ] {
            assert!(
                sample_source_alignment(initial, observed, SOURCE_SAMPLE_TIMEOUT, || {
                    std::future::ready(Ok(invalid))
                })
                .await
                .is_err()
            );
        }
        let error = sample_source_alignment(initial, observed, SOURCE_SAMPLE_TIMEOUT, || {
            std::future::ready(Err(anyhow::anyhow!("source unavailable")))
        })
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "source unavailable");
    }

    #[tokio::test]
    async fn deadline_covers_stale_samples_and_an_unresponsive_read() {
        let initial = sample("2026-08-05T12:00:00Z", 100.0);
        let observed = initial.updated_at + chrono::Duration::seconds(2);
        let timeout = Duration::from_millis(20);
        let stale = sample_source_alignment(initial, observed, timeout, || {
            std::future::ready(Ok(initial))
        })
        .await
        .unwrap_err();
        assert!(stale.to_string().contains("did not bracket"));
        let unresponsive = sample_source_alignment(initial, observed, timeout, || {
            std::future::pending::<Result<SourceTimelineSample>>()
        })
        .await
        .unwrap_err();
        assert!(unresponsive.to_string().contains("did not bracket"));
    }

    #[test]
    fn source_admission_rejects_stopped_or_malformed_state() {
        for state in [
            serde_json::json!({"lifecycle":"stopped", "updated_at":"2026-08-05T12:00:00Z", "simulation_time_s":100}),
            serde_json::json!({"lifecycle":"running", "updated_at":"invalid", "simulation_time_s":100}),
            serde_json::json!({"lifecycle":"running", "updated_at":"2026-08-05T12:00:00Z", "simulation_time_s":-1}),
            serde_json::json!({"lifecycle":"running", "updated_at":"2026-08-05T12:00:00Z"}),
        ] {
            assert!(source_timeline_sample(&state).is_err());
        }
    }
}
