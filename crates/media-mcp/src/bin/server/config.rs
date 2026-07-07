use std::{num::NonZeroU32, path::PathBuf};

use chrono::{DateTime, TimeDelta, Utc};
use clap::Parser;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_media_mcp::provider::DEFAULT_BASE_URL;

#[derive(Parser, Debug)]
#[command(name = "server", about = "Media MCP server (streamable HTTP)")]
pub(super) struct Args {
    /// Port to bind on 0.0.0.0.
    #[arg(long, default_value_t = 8787)]
    pub(super) port: u16,
    /// Public base URL reachable by the media provider.
    /// Required because media task completion is webhook-only.
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub(super) public_base_url: String,
    /// Directory served at /media/files/* so the media provider can fetch input media by URL.
    #[arg(long)]
    pub(super) static_dir: Option<PathBuf>,
    /// DuckDB state database path for task, prediction, artifact, and usage metadata.
    #[arg(long, default_value = "state.duckdb")]
    pub(super) state_db: PathBuf,
    /// Base URL of the shared artifact-plane service. All artifact reads/writes
    /// go here; media no longer owns a private bucket.
    #[arg(long, default_value = "http://artifact-service:8790")]
    pub(super) artifact_service_url: String,
    /// Allow loopback Host headers for local development and smoke tests.
    #[arg(long, default_value_t = false)]
    pub(super) allow_loopback_hosts: bool,
    /// Additional exact Host authorities trusted for private service-to-service traffic.
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    pub(super) allowed_hosts: Vec<String>,
    #[arg(long, env = "MEDIA_PROVIDER_API_KEY", hide_env_values = true)]
    api_key: Option<String>,
    /// Provider API base URL. Hidden because the concrete provider is an implementation detail.
    #[arg(long, default_value = DEFAULT_BASE_URL, hide = true)]
    pub(super) provider_base_url: String,
    #[arg(long, env = "MEDIA_PROVIDER_WEBHOOK_SECRET", hide_env_values = true)]
    webhook_secret: Option<String>,
    /// Secret used to verify gateway-to-server internal identity assertions.
    #[arg(long, env = "VEOVEO_INTERNAL_TOKEN_SECRET", hide_env_values = true)]
    pub(super) internal_token_secret: String,
    /// Retention window for completed task metadata.
    #[arg(long, default_value = "30", value_parser = clap::value_parser!(NonZeroU32))]
    task_metadata_retention_days: NonZeroU32,
    /// Retention window for artifact metadata.
    #[arg(long, default_value = "30", value_parser = clap::value_parser!(NonZeroU32))]
    artifact_metadata_retention_days: NonZeroU32,
    /// Retention window for artifact bytes.
    #[arg(long, default_value = "30", value_parser = clap::value_parser!(NonZeroU32))]
    artifact_bytes_retention_days: NonZeroU32,
    /// Retention window for usage analytics.
    #[arg(long, default_value = "365", value_parser = clap::value_parser!(NonZeroU32))]
    usage_analytics_retention_days: NonZeroU32,
}

fn parse_allowed_host(value: &str) -> Result<String, String> {
    let value = value.trim();
    parse_allowed_host_authority(value)
        .map(|_| value.to_string())
        .ok_or_else(|| "expected a host authority such as media-mcp:8787".to_string())
}

impl Args {
    pub(super) fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }

    pub(super) fn provider_api_key(&self) -> anyhow::Result<&str> {
        self.api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing MEDIA_PROVIDER_API_KEY"))
    }

    pub(super) fn provider_webhook_secret(&self) -> Option<String> {
        self.webhook_secret
            .as_deref()
            .filter(|secret| !secret.is_empty())
            .map(str::to_string)
    }

    pub(super) fn retention_policy(&self) -> MediaRetentionPolicy {
        MediaRetentionPolicy {
            task_metadata_days: self.task_metadata_retention_days,
            artifact_metadata_days: self.artifact_metadata_retention_days,
            artifact_bytes_days: self.artifact_bytes_retention_days,
            usage_analytics_days: self.usage_analytics_retention_days,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MediaRetentionPolicy {
    task_metadata_days: NonZeroU32,
    artifact_metadata_days: NonZeroU32,
    artifact_bytes_days: NonZeroU32,
    usage_analytics_days: NonZeroU32,
}

impl MediaRetentionPolicy {
    pub(super) fn task_cutoff(self, now: DateTime<Utc>) -> anyhow::Result<DateTime<Utc>> {
        retention_cutoff(now, self.task_metadata_days)
    }

    pub(super) fn artifact_expires_at(self, now: DateTime<Utc>) -> anyhow::Result<DateTime<Utc>> {
        retention_expires_at(
            now,
            std::cmp::min(self.artifact_metadata_days, self.artifact_bytes_days),
        )
    }

    pub(super) fn usage_cutoff(self, now: DateTime<Utc>) -> anyhow::Result<DateTime<Utc>> {
        retention_cutoff(now, self.usage_analytics_days)
    }
}

fn retention_cutoff(now: DateTime<Utc>, days: NonZeroU32) -> anyhow::Result<DateTime<Utc>> {
    now.checked_sub_signed(TimeDelta::days(i64::from(days.get())))
        .ok_or_else(|| anyhow::anyhow!("retention cutoff overflow for {days} day window"))
}

fn retention_expires_at(now: DateTime<Utc>, days: NonZeroU32) -> anyhow::Result<DateTime<Utc>> {
    now.checked_add_signed(TimeDelta::days(i64::from(days.get())))
        .ok_or_else(|| anyhow::anyhow!("retention expiration overflow for {days} day window"))
}
