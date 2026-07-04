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

use anyhow::{Context, anyhow};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    GatewayControlPlaneRevision, GatewayControlPlaneRevisionId, GatewayControlPlaneRevisionSource,
    PrincipalId, SecretPurpose, SecretReferenceId, SecretSource, TelemetryGuard,
    init_server_telemetry,
};
use veoveo_mcp_gateway::{GatewayCatalog, GatewayControlDb, GatewaySecretResolver, GatewayState};

use runtime::{GatewayRetentionPolicy, RuntimeControlPlaneSource};

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
    /// Seed Postgres with a validated gateway control-plane revision and make it active.
    ControlPlaneSeed {
        /// Gateway control-plane Postgres URL.
        #[arg(long, env = "VEOVEO_GATEWAY_CONTROL_DB_URL", hide_env_values = true)]
        control_db_url: String,
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
        /// Principal id recorded as the seeding actor.
        #[arg(long)]
        applied_by: String,
    },
    /// Validate the active gateway control-plane revision stored in Postgres.
    ValidateDb {
        /// Gateway control-plane Postgres URL.
        #[arg(long, env = "VEOVEO_GATEWAY_CONTROL_DB_URL", hide_env_values = true)]
        control_db_url: String,
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
    /// Print gateway auth audit counts grouped by one metadata value as JSON.
    AuthAuditMetadataSummary {
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
        /// Metadata key to group by.
        #[arg(long)]
        metadata_key: String,
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
        /// Authoritative source for gateway control data.
        #[arg(long, value_enum)]
        control_plane_source: ControlPlaneSourceArg,
        /// JSON control plane file. Required when --control-plane-source=file.
        #[arg(long)]
        control_plane: Option<PathBuf>,
        /// Gateway control-plane Postgres URL. Required when --control-plane-source=postgres.
        #[arg(long, env = "VEOVEO_GATEWAY_CONTROL_DB_URL", hide_env_values = true)]
        control_db_url: Option<String>,
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

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ControlPlaneSourceArg {
    File,
    Postgres,
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
        Command::ControlPlaneSeed {
            control_db_url,
            control_plane,
            applied_by,
        } => {
            let catalog = GatewayCatalog::load_json(&control_plane)?;
            let control_plane = catalog.control_plane().clone();
            let sha256 = control_plane_sha256(&control_plane)?;
            let revision_id =
                GatewayControlPlaneRevisionId::new(format!("gcp-{}", uuid::Uuid::new_v4()))?;
            let revision = GatewayControlPlaneRevision {
                revision_id: revision_id.clone(),
                sha256: sha256.clone(),
                source: GatewayControlPlaneRevisionSource::SeedFile,
                applied_at: Utc::now(),
                applied_by: PrincipalId::new(applied_by)?,
                tenant: None,
                control_plane,
            };
            let db = GatewayControlDb::connect(control_db_url).await?;
            db.migrate().await?;
            db.record_revision(&revision).await?;
            println!(
                "{}",
                serde_json::to_string(&ControlPlaneSeedResult {
                    status: "seeded",
                    revision_id: revision_id.to_string(),
                    sha256,
                    servers: catalog.server_count(),
                    profiles: catalog.profile_count(),
                    revisions: db.revision_count().await?,
                    active_objects: db.object_count_for_active_revision().await?,
                })?
            );
            Ok(())
        }
        Command::ValidateDb { control_db_url } => {
            let db = GatewayControlDb::connect(control_db_url).await?;
            db.migrate().await?;
            let revision = db
                .load_active_revision()
                .await?
                .context("gateway control-plane Postgres has no active revision")?;
            let catalog = GatewayCatalog::from_control_plane(revision.control_plane)?;
            println!(
                "ok: revision {}, {} server(s), {} profile(s)",
                revision.revision_id,
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
        Command::AuthAuditMetadataSummary {
            state_db,
            metadata_key,
        } => {
            let state = GatewayState::open(&state_db)?;
            println!(
                "{}",
                serde_json::to_string(&state.auth_audit_metadata_summary(&metadata_key)?)?
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
            control_plane_source,
            control_plane,
            control_db_url,
            state_db,
            internal_token_secret,
            allow_loopback_hosts,
            audit_event_retention_days,
        } => {
            let control_plane =
                runtime_control_plane_source(control_plane_source, control_plane, control_db_url)
                    .await?;
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

async fn runtime_control_plane_source(
    source: ControlPlaneSourceArg,
    control_plane: Option<PathBuf>,
    control_db_url: Option<String>,
) -> anyhow::Result<RuntimeControlPlaneSource> {
    match source {
        ControlPlaneSourceArg::File => Ok(RuntimeControlPlaneSource::File {
            path: control_plane.context("--control-plane is required for file control source")?,
        }),
        ControlPlaneSourceArg::Postgres => {
            if control_plane.is_some() {
                return Err(anyhow!(
                    "--control-plane is not accepted when --control-plane-source=postgres"
                ));
            }
            Ok(RuntimeControlPlaneSource::Postgres {
                db: GatewayControlDb::connect(control_db_url.context(
                    "--control-db-url or VEOVEO_GATEWAY_CONTROL_DB_URL is required for postgres control source",
                )?)
                .await?,
            })
        }
    }
}

fn control_plane_sha256(
    control_plane: &veoveo_mcp_contract::GatewayControlPlane,
) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(control_plane)?;
    Ok(sha256_hex(&bytes))
}

#[derive(Debug, Serialize)]
struct ControlPlaneSeedResult {
    status: &'static str,
    revision_id: String,
    sha256: String,
    servers: usize,
    profiles: usize,
    revisions: u64,
    active_objects: u64,
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
