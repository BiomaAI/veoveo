//! Typed, fail-closed spooler configuration.
//!
//! Routing maps a producer's Rerun application id to a dataset (a directory of
//! day-partitioned segment files). Routes are longest-prefix matched, and an
//! unmatched producer lands in the `quarantine` dataset rather than being
//! dropped — nothing a sensor sends is ever silently lost.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

/// A validated dataset name: lowercase, path-safe, non-empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DatasetName(String);

impl DatasetName {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        ensure!(!raw.is_empty(), "dataset name must not be empty");
        ensure!(
            raw.len() <= 64,
            "dataset name `{raw}` exceeds 64 characters"
        );
        ensure!(
            raw.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "dataset name `{raw}` must be lowercase [a-z0-9_]"
        );
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for DatasetName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for DatasetName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Route a producer application id (by prefix) to a dataset.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetRoute {
    pub dataset: DatasetName,
    /// Longest matching prefix wins. An empty prefix is the catch-all.
    pub application_id_prefix: String,
}

/// The dataset unmatched producers land in.
pub const QUARANTINE_DATASET: &str = "quarantine";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpoolerConfig {
    /// gRPC ingest bind address (the embedded proxy).
    pub bind: SocketAddr,
    /// Root directory that holds `{dataset}/{day}/{recording}.rrd`.
    pub spool_dir: PathBuf,
    /// Routing table, longest-prefix matched.
    #[serde(default)]
    pub datasets: Vec<DatasetRoute>,
    /// Freeze a live segment once it exceeds this size.
    #[serde(default = "default_segment_max_bytes")]
    pub segment_max_bytes: u64,
    /// Freeze a live segment once it is older than this (seconds).
    #[serde(default = "default_segment_max_age_s")]
    pub segment_max_age_s: u64,
    /// Flush buffered writes to the OS at most this often (milliseconds).
    #[serde(default = "default_flush_interval_ms")]
    pub flush_interval_ms: u64,
    /// In-memory replay-buffer limit for late-joining live viewers (bytes).
    #[serde(default = "default_live_queue_limit_bytes")]
    pub live_queue_limit_bytes: u64,
    /// Path to the `rerun` CLI used for freeze verify/optimize; `None` skips it.
    #[serde(default)]
    pub rerun_bin: Option<PathBuf>,
}

fn default_segment_max_bytes() -> u64 {
    256 * 1024 * 1024
}
fn default_segment_max_age_s() -> u64 {
    3600
}
fn default_flush_interval_ms() -> u64 {
    250
}
fn default_live_queue_limit_bytes() -> u64 {
    1024 * 1024 * 1024
}

impl SpoolerConfig {
    /// Validate invariants that must hold before the spooler accepts traffic.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.segment_max_bytes >= 4096,
            "segment_max_bytes must be at least 4096"
        );
        ensure!(
            self.segment_max_age_s >= 1,
            "segment_max_age_s must be at least 1"
        );
        ensure!(
            self.flush_interval_ms >= 1,
            "flush_interval_ms must be at least 1"
        );
        // Reject ambiguous routing: two routes with the same prefix.
        let mut prefixes: Vec<&str> = self
            .datasets
            .iter()
            .map(|r| r.application_id_prefix.as_str())
            .collect();
        prefixes.sort_unstable();
        for pair in prefixes.windows(2) {
            if pair[0] == pair[1] {
                bail!("duplicate routing prefix `{}`", pair[0]);
            }
        }
        Ok(())
    }

    pub fn flush_interval(&self) -> Duration {
        Duration::from_millis(self.flush_interval_ms)
    }

    pub fn segment_max_age(&self) -> Duration {
        Duration::from_secs(self.segment_max_age_s)
    }

    /// Resolve the dataset for a producer application id by longest-prefix match,
    /// falling back to the quarantine dataset.
    pub fn dataset_for(&self, application_id: &str) -> DatasetName {
        self.datasets
            .iter()
            .filter(|route| application_id.starts_with(&route.application_id_prefix))
            .max_by_key(|route| route.application_id_prefix.len())
            .map(|route| route.dataset.clone())
            .unwrap_or_else(|| {
                DatasetName::new(QUARANTINE_DATASET).expect("quarantine is a valid dataset name")
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(routes: Vec<DatasetRoute>) -> SpoolerConfig {
        SpoolerConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            spool_dir: PathBuf::from("/tmp/spool"),
            datasets: routes,
            segment_max_bytes: default_segment_max_bytes(),
            segment_max_age_s: default_segment_max_age_s(),
            flush_interval_ms: default_flush_interval_ms(),
            live_queue_limit_bytes: default_live_queue_limit_bytes(),
            rerun_bin: None,
        }
    }

    fn route(dataset: &str, prefix: &str) -> DatasetRoute {
        DatasetRoute {
            dataset: DatasetName::new(dataset).unwrap(),
            application_id_prefix: prefix.to_string(),
        }
    }

    #[test]
    fn dataset_name_rejects_invalid() {
        assert!(DatasetName::new("").is_err());
        assert!(DatasetName::new("World").is_err());
        assert!(DatasetName::new("a b").is_err());
        assert!(DatasetName::new("world").is_ok());
        assert!(DatasetName::new("world_2").is_ok());
    }

    #[test]
    fn longest_prefix_wins() {
        let config = cfg(vec![
            route("world", ""),
            route("agents", "veoveo-agent"),
            route("sumo", "veoveo-sumo"),
        ]);
        assert_eq!(config.dataset_for("veoveo-agent-pilot").as_str(), "agents");
        assert_eq!(config.dataset_for("veoveo-sumo-run-1").as_str(), "sumo");
        assert_eq!(config.dataset_for("veoveo-sim-imu").as_str(), "world");
    }

    #[test]
    fn unmatched_lands_in_quarantine() {
        let config = cfg(vec![route("agents", "veoveo-agent")]);
        assert_eq!(config.dataset_for("mystery-device").as_str(), "quarantine");
    }

    #[test]
    fn duplicate_prefixes_rejected() {
        let config = cfg(vec![route("world", "x"), route("agents", "x")]);
        assert!(config.validate().is_err());
    }

    #[test]
    fn valid_config_passes() {
        let config = cfg(vec![route("world", ""), route("agents", "veoveo-agent")]);
        assert!(config.validate().is_ok());
    }
}
