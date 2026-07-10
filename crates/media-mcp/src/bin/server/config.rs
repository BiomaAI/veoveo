use std::{convert::Infallible, fmt, num::NonZeroU32, path::PathBuf, str::FromStr};

use chrono::{DateTime, TimeDelta, Utc};
use clap::Parser;
use secrecy::{ExposeSecret, SecretString};
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_media_mcp::provider::DEFAULT_BASE_URL;
use veoveo_task_runtime::StoreAuthLevel;

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
    api_key: RedactedSecret,
    /// Provider API base URL. Hidden because the concrete provider is an implementation detail.
    #[arg(long, default_value = DEFAULT_BASE_URL, hide = true)]
    pub(super) provider_base_url: String,
    #[arg(long, env = "MEDIA_PROVIDER_WEBHOOK_SECRET", hide_env_values = true)]
    webhook_secret: RedactedSecret,
    #[arg(long = "surreal-endpoint", env = "VEOVEO_SURREAL_ENDPOINT")]
    pub(super) surreal_endpoint: String,
    #[arg(long = "surreal-namespace", env = "VEOVEO_SURREAL_NAMESPACE")]
    pub(super) surreal_namespace: String,
    #[arg(long = "surreal-database", env = "VEOVEO_SURREAL_DATABASE")]
    pub(super) surreal_database: String,
    #[arg(
        long = "surreal-auth-level",
        env = "VEOVEO_SURREAL_AUTH_LEVEL",
        value_parser = parse_database_auth_level
    )]
    pub(super) surreal_auth_level: StoreAuthLevel,
    #[arg(long = "surreal-username", env = "VEOVEO_SURREAL_USERNAME")]
    pub(super) surreal_username: String,
    #[arg(
        long = "surreal-password",
        env = "VEOVEO_SURREAL_PASSWORD",
        hide_env_values = true
    )]
    surreal_password: RedactedSecret,
    /// Public Ed25519 JWKS used to verify gateway identity assertions.
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    pub(super) internal_trust_jwks: String,
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

fn parse_database_auth_level(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse::<StoreAuthLevel>() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        Ok(_) => Err("media requires database-scoped SurrealDB credentials".into()),
        Err(error) => Err(error.to_string()),
    }
}

impl Args {
    pub(super) fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }

    pub(super) fn provider_api_key(&self) -> anyhow::Result<SecretString> {
        (!self.api_key.expose_secret().trim().is_empty())
            .then(|| self.api_key.clone_secret())
            .ok_or_else(|| anyhow::anyhow!("MEDIA_PROVIDER_API_KEY must not be empty"))
    }

    pub(super) fn provider_webhook_secret(&self) -> anyhow::Result<SecretString> {
        (!self.webhook_secret.expose_secret().trim().is_empty())
            .then(|| self.webhook_secret.clone_secret())
            .ok_or_else(|| anyhow::anyhow!("MEDIA_PROVIDER_WEBHOOK_SECRET must not be empty"))
    }

    pub(super) fn surreal_password(&self) -> SecretString {
        self.surreal_password.clone_secret()
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

#[derive(Clone)]
struct RedactedSecret(SecretString);

impl RedactedSecret {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }

    fn clone_secret(&self) -> SecretString {
        self.0.clone()
    }
}

impl fmt::Debug for RedactedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl FromStr for RedactedSecret {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(SecretString::from(value)))
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
    pub(super) fn task_ttl_ms(self) -> u64 {
        u64::from(self.task_metadata_days.get()) * 24 * 60 * 60 * 1_000
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_debug_redacts_all_secret_values() {
        let args = Args::try_parse_from([
            "server",
            "--public-base-url",
            "https://veoveo.example.com",
            "--api-key",
            "provider-api-key-sentinel",
            "--webhook-secret",
            "provider-webhook-secret-sentinel",
            "--surreal-endpoint",
            "wss://surreal.example.com/rpc",
            "--surreal-namespace",
            "veoveo",
            "--surreal-database",
            "platform",
            "--surreal-auth-level",
            "database",
            "--surreal-username",
            "runtime",
            "--surreal-password",
            "surreal-password-sentinel",
            "--internal-trust-jwks",
            r#"{"keys":[]}"#,
        ])
        .unwrap();

        let debug = format!("{args:?}");
        assert_eq!(debug.matches("[REDACTED]").count(), 3);
        for secret in [
            "provider-api-key-sentinel",
            "provider-webhook-secret-sentinel",
            "surreal-password-sentinel",
        ] {
            assert!(!debug.contains(secret));
        }
    }
}
