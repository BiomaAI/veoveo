use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    num::NonZeroU32,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

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
#[path = "gateway/runtime.rs"]
mod runtime;
#[path = "gateway/tokens.rs"]
mod tokens;

use anyhow::anyhow;
use axum::{
    Form, Json, Router,
    extract::{Path as AxumPath, Request, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{any, get, post},
};
use chrono::Utc;
use clap::{Parser, Subcommand};
use parking_lot::RwLock;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    AuthMode, AuthOutcome, AuthReasonCode, GATEWAY_INTERNAL_TOKEN_ISSUER,
    GatewayInternalTokenIssuer, GatewayProfile, GatewayProfileId, InternalTokenSecret,
    OAuthAuthorizationCode, OAuthClientAuthMethod, OAuthClientId, OAuthClientRegistration,
    OAuthGrantType, OAuthRedirectUri, PkceCodeChallengeMethod, PkceCodeVerifier, PrincipalKind,
    PublicDeployment, ResourceAuthorizationServer, ScopeName, TelemetryGuard, TokenIssuer,
    init_server_telemetry,
};
use veoveo_mcp_gateway::{
    ClientAssertionConfig, ClientAssertionVerifier, GatewayCatalog, GatewayCatalogHandle,
    GatewayMcp, GatewayState, IdJagConfig, IdJagVerifier,
};

use admin::{
    prune_jwt_revocations, read_control_plane, reload_control_plane, revoke_jwt,
    update_control_plane,
};
use audit::{
    AuthAuditRecord, auth_audit_error_response, internal_error_response, record_id_jag_auth_audit,
    record_oidc_auth_audit, record_token_auth_audit,
};
use auth::{
    authenticate_mcp, authorization_server_jwks, authorization_server_metadata,
    protected_resource_metadata,
};
use http_util::{
    TokenResponse, allowed_gateway_jwt_algorithms, load_jwks, oauth_error_response,
    pkce_s256_challenge, scope_string, token_response,
};
use oauth::{authorization_callback, authorize_endpoint};
use runtime::{
    AdminState, AppState, DynamicMcpState, GatewayRetentionPolicy, ProfileAuthState,
    ProfileMcpService, Readiness, build_http_client, current_catalog, current_http_client,
    profile_id_from_gateway_path, run_gateway_retention_gc, spawn_gateway_retention_gc_loop,
};
use tokens::{ACCESS_TOKEN_TTL_SECONDS, issue_access_token, issue_client_credentials_access_token};

const CLIENT_ASSERTION_TYPE_JWT_BEARER: &str =
    "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";
const JWT_BEARER_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

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

#[derive(Debug, Deserialize)]
struct TokenRequest {
    grant_type: String,
    client_id: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    redirect_uri: Option<String>,
    #[serde(default)]
    code_verifier: Option<String>,
    #[serde(default)]
    client_assertion_type: Option<String>,
    #[serde(default)]
    client_assertion: Option<String>,
    #[serde(default)]
    assertion: Option<String>,
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

async fn token_endpoint(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
    Form(request): Form<TokenRequest>,
) -> axum::response::Response {
    let started_at = Instant::now();
    let Ok(profile_id) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(&profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(authorization_server) = catalog.authorization_server(&profile.authorization_server)
    else {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: None,
                client_id: None,
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnknownAuthorizationServer,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "authorization server is unavailable",
        );
    };

    if request.grant_type == JWT_BEARER_GRANT_TYPE {
        return token_endpoint_id_jag(
            &state,
            &catalog,
            profile,
            authorization_server,
            request,
            started_at,
        )
        .await;
    }

    if request.grant_type == "authorization_code" {
        return token_endpoint_authorization_code(
            &state,
            &catalog,
            profile,
            authorization_server,
            request,
            started_at,
        )
        .await;
    }

    if request.grant_type != "client_credentials"
        || !profile
            .auth_modes
            .contains(&AuthMode::OAuthClientCredentials)
    {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: None,
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnsupportedGrantType,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "grant type is not supported for this gateway profile",
        );
    }

    let client_id = match OAuthClientId::new(request.client_id.trim()) {
        Ok(client_id) => client_id,
        Err(_) => {
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: None,
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidClient,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication failed",
            );
        }
    };
    let Some(client) = catalog.oauth_client(&client_id) else {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    };
    if client.authorization_server != profile.authorization_server
        || !client.allowed_profiles.contains(&profile.id)
        || !client
            .grant_types
            .contains(&OAuthGrantType::ClientCredentials)
        || !client
            .auth_methods
            .contains(&OAuthClientAuthMethod::PrivateKeyJwt)
    {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    }

    if request.client_assertion_type.as_deref() != Some(CLIENT_ASSERTION_TYPE_JWT_BEARER) {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClientAssertion,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    }
    let Some(assertion) = request.client_assertion.as_deref() else {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClientAssertion,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    };

    let Some(client_jwks_source) = client.jwks.as_ref() else {
        tracing::error!(client = %client_id, "private-key JWT client is missing JWKS source");
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidAuthConfig,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "client authentication is not configured",
        );
    };
    let http = current_http_client(&state.http);
    let client_jwks = match load_jwks(&http, client_jwks_source).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!(client = %client_id, "failed to load OAuth client JWKS: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::AuthorizationServerUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication failed",
            );
        }
    };
    let assertion_config = match ClientAssertionConfig::new(
        client_id.clone(),
        authorization_server.token_endpoint.as_str(),
        allowed_gateway_jwt_algorithms(),
    ) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("invalid client assertion verifier config: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthConfig,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "client authentication is not configured",
            );
        }
    };
    let verified_assertion =
        match ClientAssertionVerifier::new(assertion_config, client_jwks).verify(assertion) {
            Ok(verified) => verified,
            Err(err) => {
                tracing::warn!(client = %client_id, "rejected OAuth client assertion: {err}");
                if let Err(err) = record_token_auth_audit(
                    &state.gateway_state,
                    profile,
                    AuthAuditRecord {
                        authorization_server: Some(authorization_server),
                        client_id: Some(&client_id),
                        principal: None,
                        jwt_id: None,
                        outcome: AuthOutcome::Deny,
                        reason: AuthReasonCode::InvalidClientAssertion,
                        started_at,
                    },
                ) {
                    return auth_audit_error_response(err);
                }
                return oauth_error_response(
                    StatusCode::UNAUTHORIZED,
                    "invalid_client",
                    "client authentication failed",
                );
            }
        };
    match state.gateway_state.record_client_assertion_jti(
        &authorization_server.id,
        &client_id,
        &verified_assertion.jwt_id,
        verified_assertion.expires_at,
        Utc::now(),
    ) {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(
                client = %client_id,
                jwt_id = %verified_assertion.jwt_id,
                "rejected replayed OAuth client assertion"
            );
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: Some(&verified_assertion.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::ClientAssertionReplay,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication failed",
            );
        }
        Err(err) => {
            tracing::error!("failed to record OAuth client assertion replay state: {err}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let scopes = match requested_token_scopes(&catalog, profile, client, request.scope.as_deref()) {
        Ok(scopes) => scopes,
        Err(err) => {
            tracing::warn!(client = %client_id, "rejected token scope request: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: Some(&verified_assertion.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidScope,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_scope",
                "requested scope is not allowed",
            );
        }
    };

    let token = match issue_client_credentials_access_token(
        &catalog,
        authorization_server,
        profile,
        &client_id,
        &scopes,
    )
    .await
    {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("failed to issue client credentials access token: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: Some(&verified_assertion.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::TokenSigningKeyUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "token signing key is unavailable",
            );
        }
    };
    if let Err(err) = record_token_auth_audit(
        &state.gateway_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: None,
            jwt_id: Some(&token.jwt_id),
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
    ) {
        return auth_audit_error_response(err);
    }

    token_response(TokenResponse {
        access_token: token.access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECONDS as u64,
        scope: scopes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" "),
    })
}

async fn token_endpoint_authorization_code(
    state: &AppState,
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    request: TokenRequest,
    started_at: Instant,
) -> axum::response::Response {
    if !profile
        .auth_modes
        .contains(&AuthMode::OidcAuthorizationCodePkce)
    {
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: None,
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnsupportedGrantType,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "authorization code flow is not enabled for this gateway profile",
        );
    }
    let client_id = match OAuthClientId::new(request.client_id.trim()) {
        Ok(client_id) => client_id,
        Err(_) => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: None,
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidClient,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication failed",
            );
        }
    };
    let Some(client) = catalog.oauth_client(&client_id) else {
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    };
    if !authorization_code_client_allowed(profile, client) {
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    }
    if request.scope.is_some() {
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidScope,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_scope",
            "authorization-code token exchange cannot request new scopes",
        );
    }
    let code = match request
        .code
        .as_deref()
        .map(str::trim)
        .map(OAuthAuthorizationCode::new)
        .transpose()
    {
        Ok(Some(code)) => code,
        _ => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthorizationCode,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "authorization code is invalid",
            );
        }
    };
    let redirect_uri = match request
        .redirect_uri
        .as_deref()
        .map(str::trim)
        .map(OAuthRedirectUri::new)
        .transpose()
    {
        Ok(Some(redirect_uri)) => redirect_uri,
        _ => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthorizationRequest,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "redirect_uri is required",
            );
        }
    };
    let code_verifier = match request
        .code_verifier
        .as_deref()
        .map(str::trim)
        .map(PkceCodeVerifier::new)
        .transpose()
    {
        Ok(Some(code_verifier)) => code_verifier,
        _ => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidPkce,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "PKCE code_verifier is required",
            );
        }
    };
    let code_record = match state
        .gateway_state
        .consume_authorization_code(&code, Utc::now())
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthorizationCode,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "authorization code is invalid or expired",
            );
        }
        Err(err) => {
            tracing::error!("failed to consume gateway authorization code: {err}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let expected_challenge = match pkce_s256_challenge(&code_verifier) {
        Ok(challenge) => challenge,
        Err(err) => return internal_error_response(err),
    };
    if code_record.profile != profile.id
        || code_record.oauth_client_id != client_id
        || code_record.redirect_uri != redirect_uri
        || code_record.code_challenge_method != PkceCodeChallengeMethod::S256
        || code_record.code_challenge != expected_challenge
    {
        if let Err(err) = record_oidc_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: Some(&code_record.principal),
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidPkce,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "authorization code binding is invalid",
        );
    }
    let token = match issue_access_token(
        catalog,
        authorization_server,
        profile,
        &code_record.principal.subject,
        PrincipalKind::User,
        Some(&code_record.principal),
        &code_record.scopes,
    )
    .await
    {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("failed to issue browser authorization-code access token: {err}");
            if let Err(err) = record_oidc_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&code_record.principal),
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::TokenSigningKeyUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "token signing key is unavailable",
            );
        }
    };
    if let Err(err) = record_oidc_auth_audit(
        &state.gateway_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: Some(&code_record.principal),
            jwt_id: Some(&token.jwt_id),
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
    ) {
        return auth_audit_error_response(err);
    }
    token_response(TokenResponse {
        access_token: token.access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECONDS as u64,
        scope: scope_string(&code_record.scopes),
    })
}

async fn token_endpoint_id_jag(
    state: &AppState,
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    request: TokenRequest,
    started_at: Instant,
) -> axum::response::Response {
    if !profile
        .auth_modes
        .contains(&AuthMode::EnterpriseManagedAuthorization)
    {
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: None,
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnsupportedGrantType,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "grant type is not supported for this gateway profile",
        );
    }

    let client_id = match OAuthClientId::new(request.client_id.trim()) {
        Ok(client_id) => client_id,
        Err(_) => {
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: None,
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidClient,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication failed",
            );
        }
    };
    let Some(client) = catalog.oauth_client(&client_id) else {
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    };
    if client.authorization_server != profile.authorization_server
        || !client.allowed_profiles.contains(&profile.id)
        || !client
            .grant_types
            .contains(&OAuthGrantType::EnterpriseManagedAuthorization)
        || !client.auth_methods.contains(&OAuthClientAuthMethod::None)
    {
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidClient,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "client authentication failed",
        );
    }

    let Some(assertion) = request.assertion.as_deref() else {
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidIdentityAssertion,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "identity assertion is required",
        );
    };
    let Some(identity_provider_id) = authorization_server.identity_provider.as_ref() else {
        tracing::error!("enterprise-managed authorization requires an identity provider");
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidAuthConfig,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "identity provider is not configured",
        );
    };
    let Some(identity_provider) = catalog.identity_provider(identity_provider_id) else {
        tracing::error!(identity_provider = %identity_provider_id, "unknown identity provider");
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: None,
                jwt_id: None,
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::UnknownIdentityProvider,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "identity provider is unavailable",
        );
    };
    let http = current_http_client(&state.http);
    let idp_jwks = match load_jwks(&http, &identity_provider.jwks).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!("failed to load identity provider JWKS for ID-JAG: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::IdentityProviderUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_grant",
                "identity assertion could not be validated",
            );
        }
    };
    let id_jag_config = match IdJagConfig::new(
        identity_provider.issuer.clone(),
        authorization_server.issuer.clone(),
        profile.protected_resource.clone(),
        allowed_gateway_jwt_algorithms(),
    ) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("invalid ID-JAG verifier config: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidAuthConfig,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "identity assertion validation is not configured",
            );
        }
    };
    let verified_id_jag = match IdJagVerifier::new(id_jag_config, idp_jwks).verify(assertion) {
        Ok(verified) => verified,
        Err(err) => {
            tracing::warn!(client = %client_id, "rejected ID-JAG: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: None,
                    jwt_id: None,
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidIdentityAssertion,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_grant",
                "identity assertion could not be validated",
            );
        }
    };
    if verified_id_jag.client_id != client_id {
        tracing::warn!(
            request_client = %client_id,
            assertion_client = %verified_id_jag.client_id,
            "rejected ID-JAG with mismatched client_id"
        );
        if let Err(err) = record_id_jag_auth_audit(
            &state.gateway_state,
            profile,
            AuthAuditRecord {
                authorization_server: Some(authorization_server),
                client_id: Some(&client_id),
                principal: Some(&verified_id_jag.principal),
                jwt_id: Some(&verified_id_jag.jwt_id),
                outcome: AuthOutcome::Deny,
                reason: AuthReasonCode::InvalidIdentityAssertion,
                started_at,
            },
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_grant",
            "identity assertion could not be validated",
        );
    }
    match state.gateway_state.record_id_jag_jti(
        &authorization_server.id,
        &client_id,
        &verified_id_jag.jwt_id,
        verified_id_jag.expires_at,
        Utc::now(),
    ) {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(
                client = %client_id,
                jwt_id = %verified_id_jag.jwt_id,
                "rejected replayed ID-JAG"
            );
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&verified_id_jag.principal),
                    jwt_id: Some(&verified_id_jag.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::IdentityAssertionReplay,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_grant",
                "identity assertion has already been used",
            );
        }
        Err(err) => {
            tracing::error!("failed to record ID-JAG replay state: {err}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let scopes = match id_jag_token_scopes(
        catalog,
        profile,
        client,
        request.scope.as_deref(),
        &verified_id_jag.scopes,
    ) {
        Ok(scopes) => scopes,
        Err(err) => {
            tracing::warn!(client = %client_id, "rejected ID-JAG scope request: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&verified_id_jag.principal),
                    jwt_id: Some(&verified_id_jag.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::InvalidScope,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_scope",
                "requested scope is not allowed",
            );
        }
    };
    let token = match issue_access_token(
        catalog,
        authorization_server,
        profile,
        &verified_id_jag.principal.subject,
        PrincipalKind::User,
        Some(&verified_id_jag.principal),
        &scopes,
    )
    .await
    {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("failed to issue ID-JAG access token: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                AuthAuditRecord {
                    authorization_server: Some(authorization_server),
                    client_id: Some(&client_id),
                    principal: Some(&verified_id_jag.principal),
                    jwt_id: Some(&verified_id_jag.jwt_id),
                    outcome: AuthOutcome::Deny,
                    reason: AuthReasonCode::TokenSigningKeyUnavailable,
                    started_at,
                },
            ) {
                return auth_audit_error_response(err);
            }
            return oauth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "token signing key is unavailable",
            );
        }
    };
    if let Err(err) = record_id_jag_auth_audit(
        &state.gateway_state,
        profile,
        AuthAuditRecord {
            authorization_server: Some(authorization_server),
            client_id: Some(&client_id),
            principal: Some(&verified_id_jag.principal),
            jwt_id: Some(&token.jwt_id),
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
    ) {
        return auth_audit_error_response(err);
    }

    token_response(TokenResponse {
        access_token: token.access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TOKEN_TTL_SECONDS as u64,
        scope: scopes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" "),
    })
}

fn requested_token_scopes(
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    client: &OAuthClientRegistration,
    raw_scope: Option<&str>,
) -> anyhow::Result<BTreeSet<ScopeName>> {
    let raw_scope = raw_scope.ok_or_else(|| anyhow!("scope is required"))?;
    let scopes = raw_scope
        .split_whitespace()
        .map(ScopeName::new)
        .collect::<Result<BTreeSet<_>, _>>()?;
    if scopes.is_empty() {
        return Err(anyhow!("scope is required"));
    }
    let profile_supported_scopes = catalog.profile_supported_scopes(profile);
    if !scopes.is_subset(&client.allowed_scopes) {
        return Err(anyhow!("requested scope is not allowed for OAuth client"));
    }
    if !scopes.is_subset(&profile_supported_scopes) {
        return Err(anyhow!(
            "requested scope is not supported by gateway profile"
        ));
    }
    Ok(scopes)
}

fn id_jag_token_scopes(
    catalog: &GatewayCatalog,
    profile: &GatewayProfile,
    client: &OAuthClientRegistration,
    raw_scope: Option<&str>,
    id_jag_scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<BTreeSet<ScopeName>> {
    if id_jag_scopes.is_empty() {
        return Err(anyhow!("ID-JAG scope is required"));
    }
    let scopes = match raw_scope {
        Some(raw_scope) => {
            let scopes = raw_scope
                .split_whitespace()
                .map(ScopeName::new)
                .collect::<Result<BTreeSet<_>, _>>()?;
            if scopes.is_empty() {
                return Err(anyhow!("scope is required"));
            }
            if !scopes.is_subset(id_jag_scopes) {
                return Err(anyhow!("requested scope exceeds ID-JAG scope"));
            }
            scopes
        }
        None => id_jag_scopes.clone(),
    };
    let profile_supported_scopes = catalog.profile_supported_scopes(profile);
    if !scopes.is_subset(&client.allowed_scopes) {
        return Err(anyhow!("requested scope is not allowed for OAuth client"));
    }
    if !scopes.is_subset(&profile_supported_scopes) {
        return Err(anyhow!(
            "requested scope is not supported by gateway profile"
        ));
    }
    Ok(scopes)
}

fn authorization_code_client_allowed(
    profile: &GatewayProfile,
    client: &OAuthClientRegistration,
) -> bool {
    client.authorization_server == profile.authorization_server
        && client.allowed_profiles.contains(&profile.id)
        && client
            .grant_types
            .contains(&OAuthGrantType::AuthorizationCodePkce)
        && client.auth_methods.contains(&OAuthClientAuthMethod::None)
}
