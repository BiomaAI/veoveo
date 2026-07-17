use std::{
    convert::Infallible, fmt, fmt::Write as _, num::NonZeroU32, path::PathBuf, str::FromStr,
};

#[path = "gateway/admin.rs"]
mod admin;
#[path = "gateway/artifact_download.rs"]
mod artifact_download;
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
#[path = "gateway/recording_ingest.rs"]
mod recording_ingest;
#[path = "gateway/recording_playback.rs"]
mod recording_playback;
#[path = "gateway/runtime.rs"]
mod runtime;
#[path = "gateway/server.rs"]
mod server;
#[path = "gateway/tokens.rs"]
mod tokens;

use anyhow::Context;
use chrono::Utc;
use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    GatewayControlPlaneRevision, GatewayControlPlaneRevisionSource, PrincipalId, SecretPurpose,
    SecretReferenceId, SecretSource, TelemetryGuard, init_server_telemetry,
};
use veoveo_mcp_gateway::{
    GatewayCatalog, GatewayControlStore, GatewayRefreshDeliveryWindow, GatewaySecretResolver,
    GatewayState, RefreshTokenDeliveryCipher, new_gateway_control_plane_revision_id,
};
use veoveo_platform_store::{PlatformStore, StoreAuthLevel, StoreConfig, StoreCredentials};

use runtime::GatewayRetentionPolicy;

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
}

#[derive(Parser, Debug)]
#[command(name = "gateway", about = "Veoveo MCP gateway")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, ClapArgs, Debug)]
struct SurrealStoreArgs {
    /// SurrealDB WebSocket endpoint.
    #[arg(long = "surreal-endpoint", env = "VEOVEO_SURREAL_ENDPOINT")]
    endpoint: String,
    /// SurrealDB namespace owned by this Veoveo installation.
    #[arg(long = "surreal-namespace", env = "VEOVEO_SURREAL_NAMESPACE")]
    namespace: String,
    /// SurrealDB database containing Veoveo platform state.
    #[arg(long = "surreal-database", env = "VEOVEO_SURREAL_DATABASE")]
    database: String,
    /// SurrealDB authentication scope.
    #[arg(long = "surreal-auth-level", env = "VEOVEO_SURREAL_AUTH_LEVEL")]
    auth_level: StoreAuthLevel,
    /// SurrealDB username.
    #[arg(long = "surreal-username", env = "VEOVEO_SURREAL_USERNAME")]
    username: String,
    /// SurrealDB password.
    #[arg(
        long = "surreal-password",
        env = "VEOVEO_SURREAL_PASSWORD",
        hide_env_values = true
    )]
    password: RedactedSecret,
}

impl SurrealStoreArgs {
    fn into_config(self) -> anyhow::Result<StoreConfig> {
        Ok(StoreConfig::builder(
            self.endpoint,
            self.namespace,
            self.database,
            StoreCredentials::new(self.auth_level, self.username, self.password.0),
        )
        .build()?)
    }
}

#[derive(Clone)]
struct RedactedSecret(SecretString);

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

#[derive(Subcommand, Debug)]
enum Command {
    /// Validate typed gateway control data and exit.
    Validate {
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
    },
    /// Install the schema, rotate the database runtime user, and publish control data.
    InstallationBootstrap {
        #[command(flatten)]
        store: SurrealStoreArgs,
        /// Database-scoped username used by all runtime workloads.
        #[arg(
            long = "surreal-runtime-username",
            env = "VEOVEO_SURREAL_RUNTIME_USERNAME"
        )]
        runtime_username: String,
        /// Database-scoped password used by all runtime workloads.
        #[arg(
            long = "surreal-runtime-password",
            env = "VEOVEO_SURREAL_RUNTIME_PASSWORD",
            hide_env_values = true
        )]
        runtime_password: RedactedSecret,
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
        /// Principal id recorded as the seeding actor.
        #[arg(long)]
        applied_by: String,
    },
    /// Validate the active gateway control-plane revision in the platform store.
    ControlPlaneValidate {
        #[command(flatten)]
        store: SurrealStoreArgs,
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
        #[command(flatten)]
        store: SurrealStoreArgs,
    },
    /// Print gateway auth audit counts grouped by auth method as JSON.
    AuthAuditMethodSummary {
        #[command(flatten)]
        store: SurrealStoreArgs,
    },
    /// Print gateway auth audit counts grouped by auth reason as JSON.
    AuthAuditReasonSummary {
        #[command(flatten)]
        store: SurrealStoreArgs,
    },
    /// Print gateway auth audit counts grouped by one metadata value as JSON.
    AuthAuditMetadataSummary {
        #[command(flatten)]
        store: SurrealStoreArgs,
        /// Metadata key to group by.
        #[arg(long)]
        metadata_key: String,
    },
    /// Print gateway policy audit counts grouped by MCP method as JSON.
    AuditMethodSummary {
        #[command(flatten)]
        store: SurrealStoreArgs,
    },
    /// Print gateway policy audit counts grouped by decision reason as JSON.
    AuditReasonSummary {
        #[command(flatten)]
        store: SurrealStoreArgs,
    },
    /// Print gateway policy audit counts grouped by one metadata value as JSON.
    AuditMetadataSummary {
        #[command(flatten)]
        store: SurrealStoreArgs,
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
        /// Private artifact service base URL used by the authorized download proxy.
        #[arg(
            long,
            env = "VEOVEO_ARTIFACT_SERVICE_URL",
            default_value = "http://artifact-service:8790"
        )]
        artifact_service_url: String,
        #[command(flatten)]
        store: SurrealStoreArgs,
        /// Base64-encoded PKCS#8 Ed25519 private key used only by the gateway
        /// to sign gateway-to-server identity assertions.
        #[arg(
            long,
            env = "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            hide_env_values = true
        )]
        internal_signing_key_der_b64: RedactedSecret,
        /// `kid` published with internal identity assertions. Rotate by
        /// distributing a trust bundle containing old and new public keys.
        #[arg(
            long,
            env = "VEOVEO_INTERNAL_SIGNING_KEY_ID",
            default_value = veoveo_mcp_contract::DEFAULT_GATEWAY_INTERNAL_SIGNING_KEY_ID
        )]
        internal_signing_key_id: String,
        /// Base64-encoded 32-byte key used only to encrypt short-lived refresh
        /// successor delivery envelopes shared by gateway replicas.
        #[arg(long, env = "VEOVEO_REFRESH_DELIVERY_KEY_B64", hide_env_values = true)]
        refresh_delivery_key_b64: RedactedSecret,
        /// Maximum time in which concurrent presentation of a consumed refresh
        /// token receives its identical successor. Delayed reuse revokes the family.
        #[arg(
            long,
            env = "VEOVEO_REFRESH_DELIVERY_WINDOW_SECONDS",
            default_value = "5",
            value_parser = clap::value_parser!(NonZeroU32)
        )]
        refresh_delivery_window_seconds: NonZeroU32,
        /// Allow loopback Host headers for local development and smoke tests.
        #[arg(long, default_value_t = false)]
        allow_loopback_hosts: bool,
        /// Installation connectivity contract.
        #[arg(
            long,
            env = "VEOVEO_CONNECTIVITY_MODE",
            value_enum,
            default_value = "connected"
        )]
        connectivity_mode: ConnectivityMode,
        /// Retention window for gateway audit evidence.
        #[arg(long, default_value = "365", value_parser = clap::value_parser!(NonZeroU32))]
        audit_event_retention_days: NonZeroU32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ConnectivityMode {
    Connected,
    Offline,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
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
        Command::InstallationBootstrap {
            store,
            runtime_username,
            runtime_password,
            control_plane,
            applied_by,
        } => {
            if store.auth_level != StoreAuthLevel::Root {
                anyhow::bail!("installation-bootstrap requires VEOVEO_SURREAL_AUTH_LEVEL=root");
            }
            let catalog = GatewayCatalog::load_json(&control_plane)?;
            let control_plane = catalog.control_plane().clone();
            let sha256 = control_plane_sha256(&control_plane)?;
            let runtime_config = StoreConfig::builder(
                &store.endpoint,
                store.namespace.clone(),
                store.database.clone(),
                StoreCredentials::database(runtime_username.clone(), runtime_password.0.clone()),
            )
            .build()?;
            let control_store = GatewayControlStore::connect(store.into_config()?).await?;
            control_store.migrate().await?;
            control_store
                .platform_store()
                .replace_database_editor(&runtime_username, &runtime_password.0)
                .await?;

            let (status, revision_id) = match control_store.load_active_revision_head().await? {
                Some(active) if active.sha256 == sha256 => {
                    let active = control_store.load_active_revision().await?.context(
                        "active gateway control plane matched the seed hash but failed validation",
                    )?;
                    ("unchanged", active.revision_id)
                }
                _ => {
                    let revision_id = new_gateway_control_plane_revision_id()?;
                    let revision = GatewayControlPlaneRevision {
                        revision_id: revision_id.clone(),
                        sha256: sha256.clone(),
                        source: GatewayControlPlaneRevisionSource::SeedFile,
                        applied_at: Utc::now(),
                        applied_by: PrincipalId::new(applied_by)?,
                        tenant: None,
                        control_plane,
                    };
                    control_store.record_revision(&revision).await?;
                    ("bootstrapped", revision_id)
                }
            };
            PlatformStore::connect(runtime_config)
                .await?
                .healthcheck()
                .await?;
            println!(
                "{}",
                serde_json::to_string(&InstallationBootstrapResult {
                    status,
                    revision_id: revision_id.to_string(),
                    sha256,
                    servers: catalog.server_count(),
                    profiles: catalog.profile_count(),
                    revisions: control_store.revision_count().await?,
                    active_objects: control_store.object_count_for_active_revision().await?,
                    runtime_auth_verified: true,
                })?
            );
            Ok(())
        }
        Command::ControlPlaneValidate { store } => {
            let store = GatewayControlStore::connect(store.into_config()?).await?;
            let revision = store
                .load_active_revision()
                .await?
                .context("SurrealDB platform store has no active gateway control-plane revision")?;
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
        Command::AuditCounts { store } => {
            let state = GatewayState::connect(store.into_config()?).await?;
            println!("{}", serde_json::to_string(&state.audit_counts().await?)?);
            Ok(())
        }
        Command::AuthAuditMethodSummary { store } => {
            let state = GatewayState::connect(store.into_config()?).await?;
            println!(
                "{}",
                serde_json::to_string(&state.auth_audit_method_summary().await?)?
            );
            Ok(())
        }
        Command::AuthAuditReasonSummary { store } => {
            let state = GatewayState::connect(store.into_config()?).await?;
            println!(
                "{}",
                serde_json::to_string(&state.auth_audit_reason_summary().await?)?
            );
            Ok(())
        }
        Command::AuthAuditMetadataSummary {
            store,
            metadata_key,
        } => {
            let state = GatewayState::connect(store.into_config()?).await?;
            println!(
                "{}",
                serde_json::to_string(&state.auth_audit_metadata_summary(&metadata_key).await?)?
            );
            Ok(())
        }
        Command::AuditMethodSummary { store } => {
            let state = GatewayState::connect(store.into_config()?).await?;
            println!(
                "{}",
                serde_json::to_string(&state.policy_audit_method_summary().await?)?
            );
            Ok(())
        }
        Command::AuditReasonSummary { store } => {
            let state = GatewayState::connect(store.into_config()?).await?;
            println!(
                "{}",
                serde_json::to_string(&state.policy_audit_reason_summary().await?)?
            );
            Ok(())
        }
        Command::AuditMetadataSummary {
            store,
            metadata_key,
        } => {
            let state = GatewayState::connect(store.into_config()?).await?;
            println!(
                "{}",
                serde_json::to_string(&state.policy_audit_metadata_summary(&metadata_key).await?)?
            );
            Ok(())
        }
        Command::Serve {
            port,
            public_base_url,
            artifact_service_url,
            store,
            internal_signing_key_der_b64,
            internal_signing_key_id,
            refresh_delivery_key_b64,
            refresh_delivery_window_seconds,
            allow_loopback_hosts,
            connectivity_mode,
            audit_event_retention_days,
        } => {
            let control_store = GatewayControlStore::connect(store.into_config()?).await?;
            let retention = GatewayRetentionPolicy {
                audit_event_days: audit_event_retention_days,
            };
            let refresh_delivery_cipher = RefreshTokenDeliveryCipher::from_base64(
                refresh_delivery_key_b64.0.expose_secret(),
            )?;
            let refresh_delivery_window =
                GatewayRefreshDeliveryWindow::from_seconds(refresh_delivery_window_seconds)?;
            server::serve(server::ServeConfig {
                port,
                public_base_url,
                artifact_service_url,
                control_store,
                internal_signing_key_der_b64: internal_signing_key_der_b64.0,
                internal_signing_key_id,
                refresh_delivery_cipher,
                refresh_delivery_window,
                allow_loopback_hosts,
                offline_mode: connectivity_mode == ConnectivityMode::Offline,
                retention,
            })
            .await
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
struct InstallationBootstrapResult {
    status: &'static str,
    revision_id: String,
    sha256: String,
    servers: usize,
    profiles: usize,
    revisions: u64,
    active_objects: u64,
    runtime_auth_verified: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    const CANONICAL_STORE_ARGS: [&str; 12] = [
        "--surreal-endpoint",
        "ws://127.0.0.1:8000",
        "--surreal-namespace",
        "veoveo",
        "--surreal-database",
        "platform",
        "--surreal-auth-level",
        "database",
        "--surreal-username",
        "root",
        "--surreal-password",
        "do-not-log",
    ];

    #[test]
    fn canonical_surreal_cli_surface_parses_and_redacts_password() {
        let mut arguments = vec!["gateway", "control-plane-validate"];
        arguments.extend(CANONICAL_STORE_ARGS);
        let parsed = Args::try_parse_from(arguments).unwrap();
        let debug = format!("{parsed:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("do-not-log"));
    }

    #[test]
    fn serve_cli_redacts_refresh_delivery_key() {
        let mut arguments = vec![
            "gateway",
            "serve",
            "--public-base-url",
            "https://veoveo.example",
            "--internal-signing-key-der-b64",
            "internal-signing-secret",
            "--refresh-delivery-key-b64",
            "refresh-delivery-secret",
        ];
        arguments.extend(CANONICAL_STORE_ARGS);
        let parsed = Args::try_parse_from(arguments).unwrap();
        let debug = format!("{parsed:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("internal-signing-secret"));
        assert!(!debug.contains("refresh-delivery-secret"));
    }
}
