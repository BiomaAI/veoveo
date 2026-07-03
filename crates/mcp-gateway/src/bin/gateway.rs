use std::{fmt::Write as _, num::NonZeroU32, path::PathBuf};

#[path = "gateway/admin.rs"]
mod admin;
#[path = "gateway/audit.rs"]
mod audit;
#[path = "gateway/auth.rs"]
mod auth;
#[path = "gateway/host.rs"]
mod host;
#[path = "gateway/http_util.rs"]
mod http_util;
#[path = "gateway/oauth.rs"]
mod oauth;
#[path = "gateway/oauth_client_credentials.rs"]
mod oauth_client_credentials;
#[path = "gateway/oauth_grants.rs"]
mod oauth_grants;
#[path = "gateway/runtime.rs"]
mod runtime;
#[path = "gateway/server.rs"]
mod server;
#[path = "gateway/tokens.rs"]
mod tokens;

use clap::{Parser, Subcommand};
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    SecretPurpose, SecretReferenceId, SecretSource, TelemetryGuard, init_server_telemetry,
};
use veoveo_mcp_gateway::{GatewayCatalog, GatewaySecretResolver, GatewayState};

use runtime::GatewayRetentionPolicy;

#[derive(Parser, Debug)]
#[command(name = "gateway", about = "Veoveo MCP gateway")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Validate typed gateway control data and exit.
    Validate {
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
    },
    /// Resolve one configured secret and print redacted evidence as JSON.
    ResolveSecret {
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
        /// Secret reference id.
        #[arg(long)]
        secret_id: String,
        /// Expected secret purpose, using the JSON control-plane value.
        #[arg(long)]
        purpose: String,
    },
    /// Print aggregate gateway audit counts as JSON.
    AuditCounts {
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
    },
    /// Print gateway auth audit counts grouped by auth method as JSON.
    AuthAuditMethodSummary {
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
    },
    /// Print gateway auth audit counts grouped by auth reason as JSON.
    AuthAuditReasonSummary {
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
    },
    /// Print gateway policy audit counts grouped by MCP method as JSON.
    AuditMethodSummary {
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
    },
    /// Print gateway policy audit counts grouped by decision reason as JSON.
    AuditReasonSummary {
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
    },
    /// Print gateway policy audit counts grouped by one metadata value as JSON.
    AuditMetadataSummary {
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
        /// Metadata key to group by.
        #[arg(long)]
        metadata_key: String,
    },
    /// Start the gateway process.
    Serve {
        /// Port to bind on 0.0.0.0.
        #[arg(long, default_value_t = 8788)]
        port: u16,
        /// Public base URL for metadata and authorization challenges.
        #[arg(long)]
        public_base_url: String,
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
        /// Secret used to sign gateway-to-server internal identity assertions.
        #[arg(long, env = "VEOVEO_INTERNAL_TOKEN_SECRET", hide_env_values = true)]
        internal_token_secret: String,
        /// Allow loopback Host headers for local development and smoke tests.
        #[arg(long, default_value_t = false)]
        allow_loopback_hosts: bool,
        /// Retention window for gateway audit evidence.
        #[arg(long, default_value = "365", value_parser = clap::value_parser!(NonZeroU32))]
        audit_event_retention_days: NonZeroU32,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-mcp-gateway", "info,veoveo_mcp_gateway=debug")?;

    match Args::parse().command {
        Command::Validate { control_plane } => {
            let catalog = GatewayCatalog::load_json(&control_plane)?;
            println!(
                "ok: {} server(s), {} profile(s)",
                catalog.server_count(),
                catalog.profile_count()
            );
            Ok(())
        }
        Command::ResolveSecret {
            control_plane,
            secret_id,
            purpose,
        } => {
            let catalog = GatewayCatalog::load_json(&control_plane)?;
            let secret_id = SecretReferenceId::new(secret_id)?;
            let purpose = parse_secret_purpose(&purpose)?;
            let resolved = GatewaySecretResolver::new()
                .resolve_string(&catalog, &secret_id, purpose)
                .await?;
            println!(
                "{}",
                serde_json::to_string(&ResolvedSecretEvidence {
                    id: resolved.id().to_string(),
                    source: resolved.source(),
                    purpose: resolved.purpose(),
                    byte_length: resolved.expose_secret().len(),
                    sha256: sha256_hex(resolved.expose_secret().as_bytes()),
                })?
            );
            Ok(())
        }
        Command::AuditCounts { state_db } => {
            let state = GatewayState::open(&state_db)?;
            println!("{}", serde_json::to_string(&state.audit_counts()?)?);
            Ok(())
        }
        Command::AuthAuditMethodSummary { state_db } => {
            let state = GatewayState::open(&state_db)?;
            println!(
                "{}",
                serde_json::to_string(&state.auth_audit_method_summary()?)?
            );
            Ok(())
        }
        Command::AuthAuditReasonSummary { state_db } => {
            let state = GatewayState::open(&state_db)?;
            println!(
                "{}",
                serde_json::to_string(&state.auth_audit_reason_summary()?)?
            );
            Ok(())
        }
        Command::AuditMethodSummary { state_db } => {
            let state = GatewayState::open(&state_db)?;
            println!(
                "{}",
                serde_json::to_string(&state.policy_audit_method_summary()?)?
            );
            Ok(())
        }
        Command::AuditReasonSummary { state_db } => {
            let state = GatewayState::open(&state_db)?;
            println!(
                "{}",
                serde_json::to_string(&state.policy_audit_reason_summary()?)?
            );
            Ok(())
        }
        Command::AuditMetadataSummary {
            state_db,
            metadata_key,
        } => {
            let state = GatewayState::open(&state_db)?;
            println!(
                "{}",
                serde_json::to_string(&state.policy_audit_metadata_summary(&metadata_key)?)?
            );
            Ok(())
        }
        Command::Serve {
            port,
            public_base_url,
            control_plane,
            state_db,
            internal_token_secret,
            allow_loopback_hosts,
            audit_event_retention_days,
        } => {
            let retention = GatewayRetentionPolicy {
                audit_event_days: audit_event_retention_days,
            };
            server::serve(
                port,
                public_base_url,
                control_plane,
                state_db,
                internal_token_secret,
                allow_loopback_hosts,
                retention,
            )
            .await
        }
    }
}

#[derive(Debug, Serialize)]
struct ResolvedSecretEvidence {
    id: String,
    source: SecretSource,
    purpose: SecretPurpose,
    byte_length: usize,
    sha256: String,
}

fn parse_secret_purpose(value: &str) -> anyhow::Result<SecretPurpose> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|err| anyhow::anyhow!("invalid secret purpose `{value}`: {err}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("write to string");
    }
    output
}
