#[path = "gateway/agent_catalog_import.rs"]
mod agent_catalog_import;
#[cfg(test)]
#[path = "gateway/managed_oauth_tests.rs"]
mod managed_oauth_tests;
#[path = "gateway/owner_modules.rs"]
mod owner_modules;
use std::{
    convert::Infallible, fmt, fmt::Write as _, num::NonZeroU32, path::PathBuf, str::FromStr,
};

#[path = "gateway/bindings.rs"]
mod bindings;
use bindings::catalog_admission;

#[path = "gateway/admin.rs"]
mod admin;
#[path = "gateway/artifact_download.rs"]
mod artifact_download;
#[path = "gateway/artifact_upload.rs"]
mod artifact_upload;
#[path = "gateway/audit.rs"]
mod audit;
#[path = "gateway/audit_cli.rs"]
mod audit_cli;
#[path = "gateway/auth.rs"]
mod auth;
#[path = "gateway/console/mod.rs"]
mod console;
#[path = "gateway/host.rs"]
mod host;
#[path = "gateway/http_util.rs"]
mod http_util;
#[path = "gateway/module_installation/mod.rs"]
mod module_installation;
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
#[cfg(test)]
#[path = "../../../../../testing/fixtures/store.rs"]
mod test_store;
#[path = "gateway/tokens.rs"]
mod tokens;

use anyhow::Context;
use chrono::Utc;
use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    GatewayControlPlaneRevision, GatewayControlPlaneRevisionSource, SecretPurpose,
    SecretReferenceId, SecretSource, TelemetryGuard, init_server_telemetry,
};
use veoveo_mcp_gateway::{
    GatewayCatalog, GatewayControlStore, GatewayRefreshDeliveryWindow, GatewaySecretResolver,
    RefreshTokenDeliveryCipher, new_gateway_control_plane_revision_id,
};
use veoveo_platform_store::{StoreAuthLevel, StoreConfig, StoreCredentials};
use veoveo_types::PrincipalId;

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
    /// Generate the selected module plan offline from this binary's compiled owner declarations.
    ModulePlan {
        #[arg(long)]
        modules: PathBuf,
        #[arg(long)]
        composition: veoveo_modules::CompositionIdentity,
        #[arg(long)]
        helm_values: bool,
    },
    /// Import a reviewed agent catalog and retained chat bindings with all gateways stopped.
    AgentCatalogImport(agent_catalog_import::Arguments),
    /// Validate typed gateway control data and exit.
    Validate {
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
    },
    /// Prepare the mixed schema and runtime credentials without publishing control data.
    InstallationPrepare {
        #[command(flatten)]
        store: SurrealStoreArgs,
        #[command(flatten)]
        plan: module_installation::PlanArgs,
        #[arg(
            long = "surreal-runtime-password",
            env = "VEOVEO_SURREAL_RUNTIME_PASSWORD",
            hide_env_values = true
        )]
        runtime_password: RedactedSecret,
        #[arg(long, default_value_t = 60)]
        wait_seconds: u64,
    },
    /// Execute one enabled module lane after its dependencies complete.
    ModuleMigrate {
        #[command(flatten)]
        store: SurrealStoreArgs,
        #[command(flatten)]
        plan: module_installation::PlanArgs,
        #[arg(long)]
        module: veoveo_modules::ModuleName,
        #[arg(long, default_value_t = 60)]
        wait_seconds: u64,
    },
    /// Read selected lane readiness without changing database state.
    ModuleStatus {
        #[command(flatten)]
        store: SurrealStoreArgs,
        #[command(flatten)]
        plan: module_installation::PlanArgs,
        #[arg(long, default_value_t = 0)]
        wait_seconds: u64,
    },
    /// Publish reviewed control data using the authenticated runtime account.
    ControlPlanePublish {
        #[command(flatten)]
        store: SurrealStoreArgs,
        #[command(flatten)]
        plan: module_installation::PlanArgs,
        #[arg(long)]
        control_plane: PathBuf,
        #[arg(long)]
        applied_by: String,
        #[arg(long, default_value_t = 60)]
        wait_seconds: u64,
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
    /// Read or verify one explicitly selected audit partition.
    Audit {
        #[command(subcommand)]
        command: audit_cli::AuditCommand,
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
        /// Seed control-plane file whose canonical revision must be active before serving.
        #[arg(long)]
        expected_control_plane: Option<PathBuf>,
        #[command(flatten)]
        plan: module_installation::PlanArgs,
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
        #[arg(long, env = "VEOVEO_AUDIT_RETENTION_DAYS", value_parser = clap::value_parser!(NonZeroU32))]
        audit_retention_days: NonZeroU32,
        /// Dedicated audit block-signing seed; never reuse an identity assertion key.
        #[arg(long, env = "VEOVEO_AUDIT_SIGNING_KEY_B64", hide_env_values = true)]
        audit_signing_key_b64: RedactedSecret,
        /// Optional public S3/OTLP destination configuration; credential values stay in Secrets.
        #[arg(long, env = "VEOVEO_AUDIT_EXPORT_CONFIG")]
        audit_export_config: Option<std::path::PathBuf>,
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
    let command = Args::parse().command;
    let _telemetry: Option<TelemetryGuard> = if matches!(&command, Command::Serve { .. }) {
        Some(init_server_telemetry(
            "veoveo-mcp-gateway",
            "info,veoveo_mcp_gateway=debug",
        )?)
    } else {
        // Operator commands reserve stdout for their result, even inside a Pod
        // whose environment configures the hosted service's OTLP exporters.
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "warn".into()),
            )
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .try_init()
            .map_err(|error| anyhow::anyhow!("initialize CLI logging: {error}"))?;
        None
    };

    match command {
        Command::ModulePlan {
            modules,
            composition,
            helm_values,
        } => module_installation::generate(&modules, composition, helm_values),
        Command::AgentCatalogImport(args) => agent_catalog_import::run(args).await,
        Command::Validate { control_plane } => {
            let catalog = GatewayCatalog::load_json(&control_plane, crate::catalog_admission()?)?;
            println!(
                "ok: {} server(s), {} profile(s)",
                catalog.server_count(),
                catalog.profile_count()
            );
            Ok(())
        }
        Command::InstallationPrepare {
            store,
            plan,
            runtime_password,
            wait_seconds,
        } => module_installation::prepare(plan, store, runtime_password, wait_seconds).await,
        Command::ModuleMigrate {
            store,
            plan,
            module,
            wait_seconds,
        } => module_installation::migrate(plan, store, module, wait_seconds).await,
        Command::ModuleStatus {
            store,
            plan,
            wait_seconds,
        } => module_installation::status(plan, store, wait_seconds).await,
        Command::ControlPlanePublish {
            store,
            plan,
            control_plane,
            applied_by,
            wait_seconds,
        } => {
            let applied_by = PrincipalId::new(applied_by)?;
            // Audit records identify the database-authenticated operator. The
            // revision's applied_by option is operator-supplied attribution.
            let audit_context = veoveo_mcp_contract::audit::AuditContext {
                actor: veoveo_mcp_contract::audit::AuditActor {
                    principal: PrincipalId::new(store.username.clone())?,
                    kind: veoveo_mcp_contract::audit::AuditPrincipalKind::Service,
                    tenant: None,
                    oauth_client: None,
                    session_family: None,
                    delegating_principal: None,
                    managed_agent: None,
                },
                authority: Default::default(),
                request: veoveo_mcp_contract::audit::AuditRequest::background(),
            };
            let catalog = GatewayCatalog::load_json(&control_plane, crate::catalog_admission()?)?;
            let control_plane = catalog.control_plane().clone();
            let sha256 = control_plane_sha256(&control_plane)?;
            plan.load()?;
            let control_store = GatewayControlStore::from_platform_store(
                module_installation::wait_current(&plan, store.into_config()?, wait_seconds)
                    .await?,
                catalog.admission(),
            );

            let preparation = module_installation::preparation_key(&plan, &plan.load()?)?;
            let (status, revision_id) = match control_store
                .load_installation_revision(&preparation)
                .await?
            {
                Some(active) if active.sha256 == sha256 => {
                    anyhow::ensure!(
                        active.control_plane == control_plane,
                        "active control-plane hash matches the seed but the validated document differs"
                    );
                    ("unchanged", active.revision_id)
                }
                _ => {
                    let revision_id = new_gateway_control_plane_revision_id()?;
                    let revision = GatewayControlPlaneRevision {
                        revision_id: revision_id.clone(),
                        sha256: sha256.clone(),
                        source: GatewayControlPlaneRevisionSource::SeedFile,
                        applied_at: Utc::now(),
                        applied_by,
                        tenant: None,
                        control_plane,
                    };
                    control_store
                        .record_installation_revision(&revision, &audit_context, &preparation)
                        .await?;
                    ("published", revision_id)
                }
            };
            println!(
                "{}",
                serde_json::to_string(&ControlPlanePublishResult {
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
            let store =
                GatewayControlStore::connect(store.into_config()?, crate::catalog_admission()?)
                    .await?;
            let revision = store
                .load_active_revision()
                .await?
                .context("SurrealDB platform store has no active gateway control-plane revision")?;
            let catalog = GatewayCatalog::from_control_plane(
                revision.control_plane,
                crate::catalog_admission()?,
            )?;
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
            let catalog = GatewayCatalog::load_json(&control_plane, crate::catalog_admission()?)?;
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
        Command::Audit { command } => command.run().await,
        Command::Serve {
            port,
            public_base_url,
            artifact_service_url,
            expected_control_plane,
            plan,
            store,
            internal_signing_key_der_b64,
            internal_signing_key_id,
            refresh_delivery_key_b64,
            refresh_delivery_window_seconds,
            allow_loopback_hosts,
            connectivity_mode,
            audit_retention_days,
            audit_signing_key_b64,
            audit_export_config,
        } => {
            plan.load()?;
            let control_store =
                GatewayControlStore::connect(store.into_config()?, crate::catalog_admission()?)
                    .await?;
            module_installation::require_current(&plan, control_store.platform_store()).await?;
            let expected_control_plane_sha256 = expected_control_plane
                .as_ref()
                .map(|path| {
                    let catalog = GatewayCatalog::load_json(path, crate::catalog_admission()?)?;
                    control_plane_sha256(catalog.control_plane())
                })
                .transpose()?;
            let audit_exports = audit_export_config
                .as_deref()
                .map(veoveo_audit::export::AuditExportConfig::load)
                .transpose()?
                .unwrap_or_default();
            let audit_signing_key =
                std::sync::Arc::new(veoveo_audit::integrity::AuditSigningKey::from_base64(
                    audit_signing_key_b64.0.expose_secret(),
                )?);
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
                expected_control_plane_sha256,
                internal_signing_key_der_b64: internal_signing_key_der_b64.0,
                internal_signing_key_id,
                refresh_delivery_cipher,
                refresh_delivery_window,
                allow_loopback_hosts,
                offline_mode: connectivity_mode == ConnectivityMode::Offline,
                audit_retention_days,
                audit_signing_key,
                audit_exports,
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
struct ControlPlanePublishResult {
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
    fn serve_cli_redacts_signing_and_delivery_keys() {
        let mut arguments = vec![
            "gateway",
            "serve",
            "--public-base-url",
            "https://veoveo.example",
            "--internal-signing-key-der-b64",
            "internal-signing-secret",
            "--refresh-delivery-key-b64",
            "refresh-delivery-secret",
            "--audit-retention-days",
            "1",
            "--audit-signing-key-b64",
            "audit-signing-secret",
        ];
        arguments.extend(CANONICAL_STORE_ARGS);
        arguments.extend([
            "--module-plan",
            "plan.json",
            "--composition",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--generation",
            "1",
            "--credential-revision",
            "fixture-v1",
            "--surreal-runtime-username",
            "runtime",
        ]);
        let parsed = Args::try_parse_from(arguments).unwrap();
        let debug = format!("{parsed:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("internal-signing-secret"));
        assert!(!debug.contains("refresh-delivery-secret"));
        assert!(!debug.contains("audit-signing-secret"));
    }
}
