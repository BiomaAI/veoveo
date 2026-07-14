use std::{collections::BTreeSet, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use tokio::io::AsyncReadExt;

use crate::contract::{ClockAssessment, ClockQuality, ClockQualityPolicy};

const OBSERVATION_LIMIT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub enum ClockSource {
    System,
    NtpdRs { observation_socket: PathBuf },
}

#[derive(Clone, Debug)]
pub struct ClockMonitor {
    source: ClockSource,
    timeout: Duration,
}

#[derive(Debug, Deserialize)]
struct NtpdObservation {
    system: NtpdSystem,
    #[serde(default)]
    sources: Vec<NtpdSource>,
}

#[derive(Debug, Deserialize)]
struct NtpdSystem {
    stratum: u8,
    #[serde(default)]
    root_delay: f64,
    #[serde(default)]
    root_variance_base: f64,
    leap_indicator: NtpdLeapIndicator,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
enum NtpdLeapIndicator {
    NoWarning,
    Leap61,
    Leap59,
    Unknown,
}

#[derive(Debug, Deserialize)]
struct NtpdSource {
    #[serde(default)]
    offset: f64,
    #[serde(default)]
    uncertainty: f64,
    #[serde(default)]
    delay: f64,
    #[serde(default)]
    unanswered_polls: u32,
    #[serde(default)]
    nts_cookies: Option<usize>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    address: String,
}

impl ClockMonitor {
    pub fn new(source: ClockSource, timeout: Duration) -> Self {
        Self { source, timeout }
    }

    pub async fn quality(&self) -> Result<ClockQuality> {
        match &self.source {
            ClockSource::System => Ok(ClockQuality {
                synchronized: false,
                estimated_offset_nanoseconds: 0,
                error_bound_nanoseconds: u64::MAX,
                stratum: 16,
                holdover_age_seconds: None,
                source_diversity: 0,
                traceability: vec!["system_clock_unmeasured".to_owned()],
                observed_at: Utc::now().to_rfc3339(),
            }),
            ClockSource::NtpdRs { observation_socket } => {
                let stream = tokio::time::timeout(
                    self.timeout,
                    tokio::net::UnixStream::connect(observation_socket),
                )
                .await
                .context("ntpd-rs observation socket timed out")??;
                let mut bytes = Vec::new();
                tokio::time::timeout(
                    self.timeout,
                    stream
                        .take(OBSERVATION_LIMIT_BYTES + 1)
                        .read_to_end(&mut bytes),
                )
                .await
                .context("ntpd-rs observation read timed out")??;
                if bytes.len() as u64 > OBSERVATION_LIMIT_BYTES {
                    anyhow::bail!("ntpd-rs observation exceeds the size limit");
                }
                let observation: NtpdObservation =
                    serde_json::from_slice(&bytes).context("decoding ntpd-rs observation")?;
                Ok(project_ntpd(observation))
            }
        }
    }
}

fn project_ntpd(observation: NtpdObservation) -> ClockQuality {
    let usable: Vec<_> = observation
        .sources
        .iter()
        .filter(|source| source.unanswered_polls < 8 && source.uncertainty.is_finite())
        .collect();
    let best = usable.iter().copied().min_by(|left, right| {
        left.uncertainty
            .total_cmp(&right.uncertainty)
            .then_with(|| left.delay.total_cmp(&right.delay))
    });
    let offset_seconds = best.map_or(0.0, |source| source.offset);
    let source_bound = best.map_or(f64::INFINITY, |source| {
        source.offset.abs() + source.uncertainty.abs() + source.delay.abs() / 2.0
    });
    let system_bound = observation.system.root_delay.abs() / 2.0
        + observation.system.root_variance_base.max(0.0).sqrt();
    let error_bound = seconds_to_u64_nanoseconds(source_bound + system_bound);
    let diversity: BTreeSet<_> = usable
        .iter()
        .map(|source| {
            if source.address.is_empty() {
                source.name.as_str()
            } else {
                source.address.as_str()
            }
        })
        .collect();
    let mut traceability = vec!["ntp".to_owned()];
    if usable.iter().any(|source| source.nts_cookies.is_some()) {
        traceability.push("nts".to_owned());
    }
    ClockQuality {
        synchronized: observation.system.stratum < 16
            && !usable.is_empty()
            && !matches!(
                observation.system.leap_indicator,
                NtpdLeapIndicator::Unknown
            ),
        estimated_offset_nanoseconds: seconds_to_i64_nanoseconds(offset_seconds),
        error_bound_nanoseconds: error_bound,
        stratum: observation.system.stratum,
        holdover_age_seconds: None,
        source_diversity: diversity.len().try_into().unwrap_or(u32::MAX),
        traceability,
        observed_at: Utc::now().to_rfc3339(),
    }
}

fn seconds_to_i64_nanoseconds(seconds: f64) -> i64 {
    if !seconds.is_finite() {
        return if seconds.is_sign_negative() {
            i64::MIN
        } else {
            i64::MAX
        };
    }
    (seconds * 1_000_000_000.0)
        .round()
        .clamp(i64::MIN as f64, i64::MAX as f64) as i64
}

fn seconds_to_u64_nanoseconds(seconds: f64) -> u64 {
    if !seconds.is_finite() || seconds < 0.0 {
        return u64::MAX;
    }
    (seconds * 1_000_000_000.0)
        .ceil()
        .clamp(0.0, u64::MAX as f64) as u64
}

pub fn assess_clock(quality: ClockQuality, policy: ClockQualityPolicy) -> ClockAssessment {
    let mut violations = Vec::new();
    if !quality.synchronized {
        violations.push("clock is not synchronized".to_owned());
    }
    if quality.error_bound_nanoseconds > policy.maximum_error_nanoseconds {
        violations.push("clock error bound exceeds policy".to_owned());
    }
    if quality.stratum > policy.maximum_stratum {
        violations.push("clock stratum exceeds policy".to_owned());
    }
    if quality.source_diversity < policy.minimum_source_diversity {
        violations.push("clock source diversity is below policy".to_owned());
    }
    if quality
        .holdover_age_seconds
        .is_some_and(|age| age > policy.maximum_holdover_seconds)
    {
        violations.push("clock holdover age exceeds policy".to_owned());
    }
    ClockAssessment {
        acceptable: violations.is_empty(),
        quality,
        policy,
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_ntpd_quality_and_enforces_policy() {
        let observation: NtpdObservation = serde_json::from_value(serde_json::json!({
            "system": {
                "stratum": 2,
                "root_delay": 0.002,
                "root_variance_base": 0.000001,
                "leap_indicator": "NoWarning"
            },
            "sources": [{
                "offset": 0.0002,
                "uncertainty": 0.0003,
                "delay": 0.004,
                "unanswered_polls": 0,
                "nts_cookies": 4,
                "name": "alpha",
                "address": "192.0.2.1:123"
            }]
        }))
        .unwrap();
        let quality = project_ntpd(observation);
        assert!(quality.synchronized);
        assert_eq!(quality.estimated_offset_nanoseconds, 200_000);
        assert_eq!(quality.source_diversity, 1);
        assert!(quality.traceability.contains(&"nts".to_owned()));
        let assessment = assess_clock(
            quality,
            ClockQualityPolicy {
                maximum_error_nanoseconds: 10_000_000,
                maximum_stratum: 4,
                minimum_source_diversity: 1,
                maximum_holdover_seconds: 60,
            },
        );
        assert!(assessment.acceptable);
    }
}
