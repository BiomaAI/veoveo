use std::{collections::BTreeMap, net::SocketAddr, num::NonZeroU32, path::PathBuf, sync::Arc};

#[path = "gateway/admin.rs"]
mod admin;
#[path = "gateway/audit.rs"]
mod audit;
#[path = "gateway/auth.rs"]
mod auth;
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
#[path = "gateway/tokens.rs"]
mod tokens;

use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{any, get, post},
};
use clap::{Parser, Subcommand};
use parking_lot::RwLock;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenIssuer, GatewayProfileId,
    InternalTokenSecret, PublicDeployment, TelemetryGuard, TokenIssuer, init_server_telemetry,
};
use veoveo_mcp_gateway::{GatewayCatalog, GatewayCatalogHandle, GatewayMcp, GatewayState};

use admin::{
    prune_jwt_revocations, read_control_plane, reload_control_plane, revoke_jwt,
    update_control_plane,
};
use auth::{
    authenticate_mcp, authorization_server_jwks, authorization_server_metadata,
    protected_resource_metadata,
};
use oauth::{authorization_callback, authorize_endpoint, token_endpoint};
use runtime::{
    AdminState, AppState, DynamicMcpState, GatewayRetentionPolicy, ProfileAuthState,
    ProfileMcpService, Readiness, build_http_client, current_catalog, profile_id_from_gateway_path,
    run_gateway_retention_gc, spawn_gateway_retention_gc_loop,
};

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
    /// Print aggregate gateway audit counts as JSON.
    AuditCounts {
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
        Command::AuditCounts { state_db } => {
            let state = GatewayState::open(&state_db)?;
            println!("{}", serde_json::to_string(&state.audit_counts()?)?);
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
            audit_event_retention_days,
        } => {
            let retention = GatewayRetentionPolicy {
                audit_event_days: audit_event_retention_days,
            };
            serve(
                port,
                public_base_url,
                control_plane,
                state_db,
                internal_token_secret,
                retention,
            )
            .await
        }
    }
}

async fn serve(
    port: u16,
    public_base_url: String,
    control_plane: PathBuf,
    state_db: PathBuf,
    internal_token_secret: String,
    retention: GatewayRetentionPolicy,
) -> anyhow::Result<()> {
    let gateway_state = veoveo_mcp_gateway::GatewayState::open(&state_db)?;
    run_gateway_retention_gc(&gateway_state, retention)?;
    spawn_gateway_retention_gc_loop(gateway_state.clone(), retention);
    let file_catalog = Arc::new(GatewayCatalog::load_json(&control_plane)?);
    let latest_revision = gateway_state.latest_control_plane_revision()?;
    let initial_catalog = if let Some(revision) = latest_revision {
        let persisted_catalog =
            Arc::new(GatewayCatalog::from_control_plane(revision.control_plane)?);
        tracing::info!(
            revision_id = %revision.revision_id,
            sha256 = %revision.sha256,
            "loaded persisted gateway control-plane revision"
        );
        persisted_catalog
    } else {
        file_catalog
    };
    let catalog = GatewayCatalogHandle::new(initial_catalog.clone());
    let internal_token_issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        InternalTokenSecret::new(internal_token_secret)?,
    );
    let deployment = PublicDeployment::new(public_base_url)?;
    let ct = CancellationToken::new();
    let allowed_hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
        deployment.host_authority().to_string(),
    ];
    let allowed_hosts = Arc::new(allowed_hosts);
    let http = Arc::new(RwLock::new(build_http_client(&initial_catalog)?));
    let state = AppState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
        http: http.clone(),
        public_base_url: deployment.base_url().to_string(),
    };

    let mut router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route("/oauth/{profile}/authorize", get(authorize_endpoint))
        .route("/oauth/{profile}/callback", get(authorization_callback))
        .route("/oauth/{profile}/token", post(token_endpoint))
        .route(
            "/.well-known/oauth-protected-resource/mcp/{profile}",
            get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/oauth/{profile}",
            get(authorization_server_metadata),
        )
        .route("/oauth/{profile}/jwks.json", get(authorization_server_jwks))
        .with_state(state);

    let auth_state = ProfileAuthState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
        public_base_url: deployment.base_url().to_string(),
        http: http.clone(),
    };
    let mcp_state = DynamicMcpState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
        internal_token_issuer: internal_token_issuer.clone(),
        allowed_hosts: allowed_hosts.clone(),
        cancellation_token: ct.child_token(),
        services: Arc::new(RwLock::new(BTreeMap::new())),
    };
    let mcp_router = Router::new()
        .route("/mcp/{profile}", any(dynamic_mcp_profile))
        .route("/mcp/{profile}/{*path}", any(dynamic_mcp_profile))
        .with_state(mcp_state)
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_mcp,
        ));
    router = router.merge(mcp_router);

    let admin_state = AdminState {
        catalog: catalog.clone(),
        http: http.clone(),
        control_plane: control_plane.clone(),
        gateway_state: gateway_state.clone(),
    };
    let admin_router = Router::new()
        .route(
            "/admin/{profile}/control-plane",
            get(read_control_plane).put(update_control_plane),
        )
        .route(
            "/admin/{profile}/reload-control-plane",
            post(reload_control_plane),
        )
        .route("/admin/{profile}/jwt-revocations", post(revoke_jwt))
        .route(
            "/admin/{profile}/jwt-revocations/prune",
            post(prune_jwt_revocations),
        )
        .with_state(admin_state)
        .layer(middleware::from_fn_with_state(auth_state, authenticate_mcp));
    router = router.merge(admin_router);
    let router = router.layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
    );

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(
        service = "veoveo-mcp-gateway",
        address = %addr,
        server_count = initial_catalog.server_count(),
        profile_count = initial_catalog.profile_count(),
        "listening"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}

async fn dynamic_mcp_profile(
    State(state): State<DynamicMcpState>,
    request: Request,
) -> axum::response::Response {
    let Some(profile_id) = profile_id_from_gateway_path(request.uri().path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    if catalog.profile(&profile_id).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    drop(catalog);

    let service = {
        let mut services = state.services.write();
        services
            .entry(profile_id.clone())
            .or_insert_with(|| build_profile_mcp_service(&state, profile_id))
            .clone()
    };
    service.handle(request).await.into_response()
}

fn build_profile_mcp_service(
    state: &DynamicMcpState,
    profile_id: GatewayProfileId,
) -> ProfileMcpService {
    let internal_token_issuer = state.internal_token_issuer.clone();
    StreamableHttpService::new(
        {
            let catalog = state.catalog.clone();
            let gateway_state = state.gateway_state.clone();
            let profile_id = profile_id.clone();
            move || {
                Ok(GatewayMcp::new(
                    catalog.clone(),
                    profile_id.clone(),
                    gateway_state.clone(),
                    internal_token_issuer.clone(),
                ))
            }
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(state.allowed_hosts.iter().cloned())
            .with_cancellation_token(state.cancellation_token.child_token()),
    )
}

async fn readyz(State(state): State<AppState>) -> Json<Readiness> {
    let catalog = current_catalog(&state.catalog);
    Json(Readiness {
        status: "ready",
        servers: catalog.server_count(),
        profiles: catalog.profile_count(),
    })
}
