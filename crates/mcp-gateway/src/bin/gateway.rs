use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use anyhow::{Context, anyhow};
use axum::{
    Form, Json, Router,
    extract::{Path as AxumPath, Request, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, PRAGMA, WWW_AUTHENTICATE},
    },
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, TimeDelta, Utc};
use clap::{Parser, Subcommand};
use jsonwebtoken::{
    Algorithm, EncodingKey, Header, encode,
    jwk::{Jwk, JwkSet},
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    AuditEvent, AuthAuditEvent, AuthMethod, AuthMode, AuthOutcome, AuthReasonCode,
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayAction, GatewayInternalTokenIssuer, GatewayJwtRevocation,
    GatewayProfile, GatewayProfileId, InternalTokenSecret, JwksSource, JwtId, McpMethodName,
    OAuthClientAuthMethod, OAuthClientId, OAuthClientRegistration, OAuthGrantType, PolicyDecision,
    PolicyEffect, PolicyTarget, Principal, PrincipalId, PrincipalKind, PublicDeployment,
    ResourceAuthorizationServer, ScopeName, SecretPurpose, SecretReferenceId, SecretSource,
    TokenIssuer, TokenSubject, TraceId,
};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, BearerToken, ClientAssertionConfig, ClientAssertionVerifier,
    GatewayCatalog, GatewayMcp, GatewayState, IdJagConfig, IdJagVerifier, JwtAuthConfig,
    JwtVerifier, PolicyRequest, www_authenticate_challenge,
};

const GATEWAY_AUTH_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const CLIENT_ASSERTION_TYPE_JWT_BEARER: &str =
    "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";
const JWT_BEARER_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";
const ACCESS_TOKEN_TTL_SECONDS: i64 = 15 * 60;
type SharedCatalog = Arc<RwLock<Arc<GatewayCatalog>>>;

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
    /// Add a gateway JWT id to the durable revocation set.
    RevokeJwt {
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
        /// Gateway profile whose protected resource accepted the JWT.
        #[arg(long)]
        profile: GatewayProfileId,
        /// Token issuer claim.
        #[arg(long)]
        issuer: TokenIssuer,
        /// JWT id claim.
        #[arg(long)]
        jwt_id: JwtId,
        /// Expiration timestamp for this revocation entry, normally the JWT exp claim.
        #[arg(long)]
        expires_at: DateTime<Utc>,
        /// Operator-readable revocation reason stored as evidence.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Remove expired gateway JWT revocation entries.
    PruneRevokedJwts {
        /// DuckDB file for gateway runtime state and audit evidence.
        #[arg(long)]
        state_db: PathBuf,
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
    },
}

#[derive(Clone)]
struct AppState {
    catalog: SharedCatalog,
    gateway_state: GatewayState,
    http: reqwest::Client,
    public_base_url: String,
}

#[derive(Clone)]
struct ProfileAuthState {
    catalog: SharedCatalog,
    gateway_state: GatewayState,
    profile_id: GatewayProfileId,
    public_base_url: String,
    http: reqwest::Client,
}

#[derive(Clone)]
struct AdminState {
    catalog: SharedCatalog,
    mounted_profiles: Arc<BTreeSet<GatewayProfileId>>,
    control_plane: PathBuf,
    gateway_state: GatewayState,
    profile_id: GatewayProfileId,
}

#[derive(Debug, Serialize)]
struct Readiness {
    status: &'static str,
    servers: usize,
    profiles: usize,
}

#[derive(Debug, Serialize)]
struct ReloadResult {
    status: &'static str,
    servers: usize,
    profiles: usize,
}

#[derive(Debug, Deserialize)]
struct TokenRequest {
    grant_type: String,
    client_id: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    client_assertion_type: Option<String>,
    #[serde(default)]
    client_assertion: Option<String>,
    #[serde(default)]
    assertion: Option<String>,
}

#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: u64,
    scope: String,
}

#[derive(Debug, Serialize)]
struct OAuthErrorResponse {
    error: &'static str,
    error_description: &'static str,
}

#[derive(Debug, Serialize)]
struct AccessTokenClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
    principal_kind: PrincipalKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    data_labels: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,gateway=debug".into()),
        )
        .init();

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
        Command::RevokeJwt {
            state_db,
            profile,
            issuer,
            jwt_id,
            expires_at,
            reason,
        } => {
            let state = GatewayState::open(&state_db)?;
            state.record_jwt_revocation(&GatewayJwtRevocation {
                profile,
                issuer,
                jwt_id,
                revoked_at: Utc::now(),
                expires_at,
                reason,
            })?;
            println!("ok");
            Ok(())
        }
        Command::PruneRevokedJwts { state_db } => {
            let state = GatewayState::open(&state_db)?;
            println!("{}", state.prune_expired_jwt_revocations(Utc::now())?);
            Ok(())
        }
        Command::Serve {
            port,
            public_base_url,
            control_plane,
            state_db,
            internal_token_secret,
        } => {
            serve(
                port,
                public_base_url,
                control_plane,
                state_db,
                internal_token_secret,
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
) -> anyhow::Result<()> {
    let initial_catalog = Arc::new(GatewayCatalog::load_json(&control_plane)?);
    let mounted_profiles = Arc::new(profile_ids(&initial_catalog));
    let catalog = Arc::new(RwLock::new(initial_catalog.clone()));
    let gateway_state = veoveo_mcp_gateway::GatewayState::open(&state_db)?;
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
    let http = reqwest::Client::builder()
        .timeout(GATEWAY_AUTH_HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let state = AppState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
        http: http.clone(),
        public_base_url: deployment.base_url().to_string(),
    };

    let mut router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
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

    for profile in initial_catalog.profiles() {
        let profile_id = profile.id.clone();
        let profile_internal_token_issuer = internal_token_issuer.clone();
        let mcp_service = StreamableHttpService::new(
            {
                let catalog = catalog.clone();
                let gateway_state = gateway_state.clone();
                let profile_id = profile_id.clone();
                move || {
                    let catalog = current_catalog(&catalog);
                    Ok(GatewayMcp::new(
                        catalog,
                        profile_id.clone(),
                        gateway_state.clone(),
                        profile_internal_token_issuer.clone(),
                    ))
                }
            },
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default()
                .with_allowed_hosts(allowed_hosts.clone())
                .with_cancellation_token(ct.child_token()),
        );
        let auth_state = ProfileAuthState {
            catalog: catalog.clone(),
            gateway_state: gateway_state.clone(),
            profile_id: profile_id.clone(),
            public_base_url: deployment.base_url().to_string(),
            http: http.clone(),
        };
        let mcp_root_service = mcp_service.clone();
        let profile_router = Router::new()
            .route_service("/", mcp_root_service)
            .route_service("/{*path}", mcp_service)
            .layer(middleware::from_fn_with_state(auth_state, authenticate_mcp));
        router = router.nest(&format!("/mcp/{profile_id}"), profile_router);

        let admin_state = AdminState {
            catalog: catalog.clone(),
            mounted_profiles: mounted_profiles.clone(),
            control_plane: control_plane.clone(),
            gateway_state: gateway_state.clone(),
            profile_id: profile_id.clone(),
        };
        let auth_state = ProfileAuthState {
            catalog: catalog.clone(),
            gateway_state: gateway_state.clone(),
            profile_id: profile_id.clone(),
            public_base_url: deployment.base_url().to_string(),
            http: http.clone(),
        };
        let admin_router = Router::new()
            .route("/reload-control-plane", post(reload_control_plane))
            .with_state(admin_state)
            .layer(middleware::from_fn_with_state(auth_state, authenticate_mcp));
        router = router.nest(&format!("/admin/{profile_id}"), admin_router);
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(
        "veoveo-mcp-gateway listening on http://{addr} with {} server(s), {} profile(s)",
        initial_catalog.server_count(),
        initial_catalog.profile_count()
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

async fn readyz(State(state): State<AppState>) -> Json<Readiness> {
    let catalog = current_catalog(&state.catalog);
    Json(Readiness {
        status: "ready",
        servers: catalog.server_count(),
        profiles: catalog.profile_count(),
    })
}

async fn protected_resource_metadata(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
) -> impl IntoResponse {
    let Ok(profile_id) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    match catalog.protected_resource_metadata(&profile_id) {
        Ok(metadata) => {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, Json(metadata)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn authorization_server_metadata(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
) -> impl IntoResponse {
    let Ok(profile_id) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    match catalog.authorization_server_metadata(&profile_id) {
        Ok(mut metadata) => {
            metadata.jwks_uri = Some(format!(
                "{}/oauth/{}/jwks.json",
                state.public_base_url.trim_end_matches('/'),
                profile_id
            ));
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, Json(metadata)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn authorization_server_jwks(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
) -> axum::response::Response {
    let Ok(profile_id) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(&profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(authorization_server) = catalog.authorization_server(&profile.authorization_server)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let jwks = match authorization_server_jwks_from_signing_key(&catalog, authorization_server) {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::error!("failed to build authorization server JWKS: {err}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300, must-revalidate"),
    );
    (StatusCode::OK, headers, Json(jwks)).into_response()
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
            None,
            None,
            None,
            AuthOutcome::Deny,
            AuthReasonCode::UnknownAuthorizationServer,
            started_at,
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

    if request.grant_type != "client_credentials"
        || !profile
            .auth_modes
            .contains(&AuthMode::OAuthClientCredentials)
    {
        if let Err(err) = record_token_auth_audit(
            &state.gateway_state,
            profile,
            Some(authorization_server),
            None,
            None,
            AuthOutcome::Deny,
            AuthReasonCode::UnsupportedGrantType,
            started_at,
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
                Some(authorization_server),
                None,
                None,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidClient,
                started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            AuthOutcome::Deny,
            AuthReasonCode::InvalidClient,
            started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            AuthOutcome::Deny,
            AuthReasonCode::InvalidClient,
            started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            AuthOutcome::Deny,
            AuthReasonCode::InvalidClientAssertion,
            started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            AuthOutcome::Deny,
            AuthReasonCode::InvalidClientAssertion,
            started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            AuthOutcome::Deny,
            AuthReasonCode::InvalidAuthConfig,
            started_at,
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "client authentication is not configured",
        );
    };
    let client_jwks = match load_jwks(&state.http, client_jwks_source).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!(client = %client_id, "failed to load OAuth client JWKS: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                Some(authorization_server),
                Some(&client_id),
                None,
                AuthOutcome::Deny,
                AuthReasonCode::AuthorizationServerUnavailable,
                started_at,
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
                Some(authorization_server),
                Some(&client_id),
                None,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidAuthConfig,
                started_at,
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
                    Some(authorization_server),
                    Some(&client_id),
                    None,
                    AuthOutcome::Deny,
                    AuthReasonCode::InvalidClientAssertion,
                    started_at,
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
                Some(authorization_server),
                Some(&client_id),
                Some(&verified_assertion.jwt_id),
                AuthOutcome::Deny,
                AuthReasonCode::ClientAssertionReplay,
                started_at,
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
                Some(authorization_server),
                Some(&client_id),
                Some(&verified_assertion.jwt_id),
                AuthOutcome::Deny,
                AuthReasonCode::InvalidScope,
                started_at,
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
    ) {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("failed to issue client credentials access token: {err}");
            if let Err(err) = record_token_auth_audit(
                &state.gateway_state,
                profile,
                Some(authorization_server),
                Some(&client_id),
                Some(&verified_assertion.jwt_id),
                AuthOutcome::Deny,
                AuthReasonCode::TokenSigningKeyUnavailable,
                started_at,
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
        Some(authorization_server),
        Some(&client_id),
        Some(&token.jwt_id),
        AuthOutcome::Allow,
        AuthReasonCode::AuthAllow,
        started_at,
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
            Some(authorization_server),
            None,
            None,
            None,
            AuthOutcome::Deny,
            AuthReasonCode::UnsupportedGrantType,
            started_at,
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
                Some(authorization_server),
                None,
                None,
                None,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidClient,
                started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            None,
            AuthOutcome::Deny,
            AuthReasonCode::InvalidClient,
            started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            None,
            AuthOutcome::Deny,
            AuthReasonCode::InvalidClient,
            started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            None,
            AuthOutcome::Deny,
            AuthReasonCode::InvalidIdentityAssertion,
            started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            None,
            AuthOutcome::Deny,
            AuthReasonCode::InvalidAuthConfig,
            started_at,
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
            Some(authorization_server),
            Some(&client_id),
            None,
            None,
            AuthOutcome::Deny,
            AuthReasonCode::UnknownIdentityProvider,
            started_at,
        ) {
            return auth_audit_error_response(err);
        }
        return oauth_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "identity provider is unavailable",
        );
    };
    let idp_jwks = match load_jwks(&state.http, &identity_provider.jwks).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!("failed to load identity provider JWKS for ID-JAG: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                Some(authorization_server),
                Some(&client_id),
                None,
                None,
                AuthOutcome::Deny,
                AuthReasonCode::IdentityProviderUnavailable,
                started_at,
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
                Some(authorization_server),
                Some(&client_id),
                None,
                None,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidAuthConfig,
                started_at,
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
                Some(authorization_server),
                Some(&client_id),
                None,
                None,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidIdentityAssertion,
                started_at,
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
            Some(authorization_server),
            Some(&client_id),
            Some(&verified_id_jag.principal),
            Some(&verified_id_jag.jwt_id),
            AuthOutcome::Deny,
            AuthReasonCode::InvalidIdentityAssertion,
            started_at,
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
                Some(authorization_server),
                Some(&client_id),
                Some(&verified_id_jag.principal),
                Some(&verified_id_jag.jwt_id),
                AuthOutcome::Deny,
                AuthReasonCode::IdentityAssertionReplay,
                started_at,
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
                Some(authorization_server),
                Some(&client_id),
                Some(&verified_id_jag.principal),
                Some(&verified_id_jag.jwt_id),
                AuthOutcome::Deny,
                AuthReasonCode::InvalidScope,
                started_at,
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
    ) {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("failed to issue ID-JAG access token: {err}");
            if let Err(err) = record_id_jag_auth_audit(
                &state.gateway_state,
                profile,
                Some(authorization_server),
                Some(&client_id),
                Some(&verified_id_jag.principal),
                Some(&verified_id_jag.jwt_id),
                AuthOutcome::Deny,
                AuthReasonCode::TokenSigningKeyUnavailable,
                started_at,
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
        Some(authorization_server),
        Some(&client_id),
        Some(&verified_id_jag.principal),
        Some(&token.jwt_id),
        AuthOutcome::Allow,
        AuthReasonCode::AuthAllow,
        started_at,
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

async fn reload_control_plane(
    State(state): State<AdminState>,
    request: Request,
) -> axum::response::Response {
    let started_at = Instant::now();
    let Some(subject) = request.extensions().get::<AuthenticatedSubject>().cloned() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(&state.profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let trace_id = match TraceId::new(uuid::Uuid::new_v4().to_string()) {
        Ok(trace_id) => trace_id,
        Err(err) => return internal_error_response(err),
    };
    let target = PolicyTarget::Gateway;
    let decision = catalog.decide(PolicyRequest {
        principal: &subject.principal,
        profile: &state.profile_id,
        action: GatewayAction::AdminWrite,
        target: &target,
        trace_id: &trace_id,
    });
    if let Err(err) = record_admin_audit(
        &state.gateway_state,
        profile,
        &subject,
        GatewayAction::AdminWrite,
        target,
        decision.clone(),
        started_at,
    ) {
        return internal_error_response(err);
    }
    if decision.effect != PolicyEffect::Allow {
        tracing::warn!(
            profile = %state.profile_id,
            principal = %subject.principal.id,
            reason = ?decision.reason,
            "gateway admin reload denied"
        );
        return StatusCode::FORBIDDEN.into_response();
    }

    let new_catalog = match GatewayCatalog::load_json(&state.control_plane) {
        Ok(catalog) => Arc::new(catalog),
        Err(err) => {
            tracing::error!("failed to reload gateway control plane: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to reload gateway control plane",
            )
                .into_response();
        }
    };
    let new_profile_ids = profile_ids(&new_catalog);
    if new_profile_ids != *state.mounted_profiles {
        tracing::error!(
            mounted = ?state.mounted_profiles,
            reloaded = ?new_profile_ids,
            "gateway control-plane reload changed mounted profile routes"
        );
        return (
            StatusCode::CONFLICT,
            "control-plane reload cannot change mounted profile routes",
        )
            .into_response();
    }

    let servers = new_catalog.server_count();
    let profiles = new_catalog.profile_count();
    replace_catalog(&state.catalog, new_catalog);
    tracing::info!(
        profile = %state.profile_id,
        principal = %subject.principal.id,
        servers,
        profiles,
        "gateway control plane reloaded"
    );
    Json(ReloadResult {
        status: "reloaded",
        servers,
        profiles,
    })
    .into_response()
}

async fn authenticate_mcp(
    State(state): State<ProfileAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let started_at = Instant::now();
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(&state.profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(authorization_server) = catalog.authorization_server(&profile.authorization_server)
    else {
        if let Err(err) = record_auth_audit(
            &state,
            profile,
            AuthOutcome::Deny,
            AuthReasonCode::UnknownAuthorizationServer,
            None,
            started_at,
        ) {
            return auth_audit_error_response(err);
        }
        return unauthorized(&state, "unknown authorization server");
    };

    let Some(header) = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        if let Err(err) = record_auth_audit(
            &state,
            profile,
            AuthOutcome::Deny,
            AuthReasonCode::MissingAuthorizationHeader,
            None,
            started_at,
        ) {
            return auth_audit_error_response(err);
        }
        return unauthorized(&state, "missing authorization header");
    };
    let token = match BearerToken::from_authorization_header(header) {
        Ok(token) => token,
        Err(err) => {
            tracing::warn!("rejected gateway request: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidAuthorizationHeader,
                None,
                started_at,
            ) {
                return auth_audit_error_response(err);
            }
            return unauthorized(&state, "invalid authorization header");
        }
    };

    let jwks = match load_jwks(&state.http, &authorization_server.jwks).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!("failed to load resource authorization server JWKS: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::AuthorizationServerUnavailable,
                None,
                started_at,
            ) {
                return auth_audit_error_response(err);
            }
            return unauthorized(&state, "authorization server unavailable");
        }
    };
    let auth_config = match JwtAuthConfig::new(
        authorization_server.issuer.clone(),
        profile.protected_resource.clone(),
        profile.required_scopes.iter().cloned().collect(),
        allowed_gateway_jwt_algorithms(),
    ) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("invalid gateway auth config: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidAuthConfig,
                None,
                started_at,
            ) {
                return auth_audit_error_response(err);
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let subject = match JwtVerifier::new(auth_config, jwks).verify(&token) {
        Ok(subject) => subject,
        Err(err) => {
            tracing::warn!("rejected gateway token: {err}");
            if let Err(err) = record_auth_audit(
                &state,
                profile,
                AuthOutcome::Deny,
                AuthReasonCode::InvalidBearerToken,
                None,
                started_at,
            ) {
                return auth_audit_error_response(err);
            }
            return unauthorized(&state, "invalid bearer token");
        }
    };
    if let Some(jwt_id) = &subject.access_token.jwt_id {
        match state.gateway_state.jwt_revocation(
            &profile.id,
            &subject.access_token.issuer,
            jwt_id,
            Utc::now(),
        ) {
            Ok(Some(_revocation)) => {
                tracing::warn!(
                    profile = %profile.id,
                    issuer = %subject.access_token.issuer,
                    jwt_id = %jwt_id,
                    "rejected revoked gateway token"
                );
                if let Err(err) = record_auth_audit(
                    &state,
                    profile,
                    AuthOutcome::Deny,
                    AuthReasonCode::TokenRevoked,
                    Some(&subject),
                    started_at,
                ) {
                    return auth_audit_error_response(err);
                }
                return unauthorized(&state, "token revoked");
            }
            Ok(None) => {}
            Err(err) => {
                tracing::error!("failed to check gateway token revocation state: {err}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }
    if let Err(err) = record_auth_audit(
        &state,
        profile,
        AuthOutcome::Allow,
        AuthReasonCode::AuthAllow,
        Some(&subject),
        started_at,
    ) {
        return auth_audit_error_response(err);
    }

    request
        .extensions_mut()
        .insert::<AuthenticatedSubject>(subject);
    next.run(request).await
}

#[derive(Debug)]
struct IssuedAccessToken {
    access_token: String,
    jwt_id: JwtId,
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

fn issue_client_credentials_access_token(
    catalog: &GatewayCatalog,
    authorization_server: &ResourceAuthorizationServer,
    profile: &GatewayProfile,
    client_id: &OAuthClientId,
    scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<IssuedAccessToken> {
    let subject = TokenSubject::new(client_id.as_str())?;
    issue_access_token(
        catalog,
        authorization_server,
        profile,
        &subject,
        PrincipalKind::Service,
        None,
        scopes,
    )
}

fn issue_access_token(
    catalog: &GatewayCatalog,
    authorization_server: &ResourceAuthorizationServer,
    profile: &GatewayProfile,
    subject: &TokenSubject,
    principal_kind: PrincipalKind,
    principal: Option<&Principal>,
    scopes: &BTreeSet<ScopeName>,
) -> anyhow::Result<IssuedAccessToken> {
    let signing_key = access_token_signing_key(
        catalog,
        &authorization_server.access_token_signing_key,
        SecretPurpose::JwksPrivateKey,
    )?;
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::seconds(ACCESS_TOKEN_TTL_SECONDS))
        .ok_or_else(|| anyhow!("access token expiration overflow"))?;
    let jwt_id = JwtId::new(uuid::Uuid::new_v4().to_string())?;
    let scope = (!scopes.is_empty()).then(|| {
        scopes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    });
    let claims = AccessTokenClaims {
        iss: authorization_server.issuer.to_string(),
        sub: subject.to_string(),
        aud: profile.protected_resource.to_string(),
        exp: unix_seconds(expires_at.timestamp())?,
        nbf: unix_seconds(now.timestamp())?,
        iat: unix_seconds(now.timestamp())?,
        jti: jwt_id.to_string(),
        principal_kind,
        scope,
        groups: principal
            .map(|principal| {
                principal
                    .groups
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        roles: principal
            .map(|principal| {
                principal
                    .roles
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        tenant: principal
            .and_then(|principal| principal.tenant.as_ref())
            .map(ToString::to_string),
        data_labels: principal
            .map(|principal| {
                principal
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(authorization_server.access_token_key_id.to_string());
    let access_token = encode(&header, &claims, &signing_key)?;
    Ok(IssuedAccessToken {
        access_token,
        jwt_id,
    })
}

fn authorization_server_jwks_from_signing_key(
    catalog: &GatewayCatalog,
    authorization_server: &ResourceAuthorizationServer,
) -> anyhow::Result<JwkSet> {
    let signing_key = access_token_signing_key(
        catalog,
        &authorization_server.access_token_signing_key,
        SecretPurpose::JwksPrivateKey,
    )?;
    let mut jwk = Jwk::from_encoding_key(&signing_key, Algorithm::RS256)?;
    jwk.common.key_id = Some(authorization_server.access_token_key_id.to_string());
    Ok(JwkSet { keys: vec![jwk] })
}

fn access_token_signing_key(
    catalog: &GatewayCatalog,
    secret_id: &SecretReferenceId,
    expected_purpose: SecretPurpose,
) -> anyhow::Result<EncodingKey> {
    let secret = catalog
        .secret_reference(secret_id)
        .ok_or_else(|| anyhow!("unknown access-token signing secret `{secret_id}`"))?;
    if secret.source != SecretSource::Env {
        return Err(anyhow!(
            "access-token signing secret `{secret_id}` uses unsupported source {:?}",
            secret.source
        ));
    }
    if secret.purpose != expected_purpose {
        return Err(anyhow!(
            "access-token signing secret `{secret_id}` has purpose {:?}, expected {:?}",
            secret.purpose,
            expected_purpose
        ));
    }
    let value = std::env::var(secret.locator.as_str())
        .with_context(|| format!("missing env secret `{}`", secret.locator))?;
    let der = BASE64_STANDARD
        .decode(value.trim())
        .context("access-token signing key must be base64-encoded RSA DER")?;
    Ok(EncodingKey::from_rsa_der(&der))
}

fn unix_seconds(value: i64) -> anyhow::Result<u64> {
    u64::try_from(value).map_err(|_| anyhow!("timestamp before Unix epoch"))
}

fn token_response(response: TokenResponse) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    (StatusCode::OK, headers, Json(response)).into_response()
}

fn oauth_error_response(
    status: StatusCode,
    error: &'static str,
    error_description: &'static str,
) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    (
        status,
        headers,
        Json(OAuthErrorResponse {
            error,
            error_description,
        }),
    )
        .into_response()
}

async fn load_jwks(http: &reqwest::Client, jwks: &JwksSource) -> anyhow::Result<JwkSet> {
    match jwks {
        JwksSource::Remote { jwks_uri } => fetch_jwks(http, jwks_uri.as_str()).await,
        JwksSource::File { path } => {
            let bytes = std::fs::read(path.as_str())?;
            Ok(serde_json::from_slice::<JwkSet>(&bytes)?)
        }
    }
}

async fn fetch_jwks(http: &reqwest::Client, url: &str) -> anyhow::Result<JwkSet> {
    let response = http.get(url).send().await?.error_for_status()?;
    Ok(response.json::<JwkSet>().await?)
}

fn current_catalog(catalog: &SharedCatalog) -> Arc<GatewayCatalog> {
    catalog
        .read()
        .expect("gateway catalog lock poisoned")
        .clone()
}

fn replace_catalog(catalog: &SharedCatalog, new_catalog: Arc<GatewayCatalog>) {
    *catalog.write().expect("gateway catalog lock poisoned") = new_catalog;
}

fn profile_ids(catalog: &GatewayCatalog) -> BTreeSet<GatewayProfileId> {
    catalog
        .profiles()
        .map(|profile| profile.id.clone())
        .collect()
}

fn record_admin_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
    action: GatewayAction,
    target: PolicyTarget,
    decision: PolicyDecision,
    started_at: Instant,
) -> anyhow::Result<()> {
    let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let latency_ms = u64::try_from(started_at.elapsed().as_millis())?;
    gateway_state.record_audit_event(&AuditEvent {
        event_id,
        timestamp: decision.evaluated_at,
        trace_id: decision.trace_id.clone(),
        profile: profile.id.clone(),
        method: McpMethodName::new("admin/reload-control-plane")?,
        action,
        target,
        decision,
        principal: Some(subject.principal.id.clone()),
        tenant: subject.principal.tenant.clone(),
        token_issuer: Some(subject.access_token.issuer.clone()),
        latency_ms: Some(latency_ms),
        metadata: BTreeMap::new(),
    })?;
    Ok(())
}

fn record_auth_audit(
    state: &ProfileAuthState,
    profile: &GatewayProfile,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    subject: Option<&AuthenticatedSubject>,
    started_at: Instant,
) -> anyhow::Result<()> {
    let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let principal = subject.map(|value| value.principal.id.clone());
    let tenant = subject.and_then(|value| value.principal.tenant.clone());
    let token_issuer = subject.map(|value| value.access_token.issuer.clone());
    let token_subject = subject.map(|value| value.access_token.subject.clone());
    let jwt_id = subject.and_then(|value| value.access_token.jwt_id.clone());
    let latency_ms = u64::try_from(started_at.elapsed().as_millis())?;
    state
        .gateway_state
        .record_auth_audit_event(&AuthAuditEvent {
            event_id,
            timestamp: chrono::Utc::now(),
            trace_id,
            profile: profile.id.clone(),
            protected_resource: profile.protected_resource.clone(),
            outcome,
            reason,
            method: AuthMethod::BearerJwt,
            principal,
            tenant,
            token_issuer,
            token_subject,
            jwt_id,
            latency_ms: Some(latency_ms),
            metadata: Default::default(),
        })
}

fn record_token_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    authorization_server: Option<&ResourceAuthorizationServer>,
    client_id: Option<&OAuthClientId>,
    jwt_id: Option<&JwtId>,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    started_at: Instant,
) -> anyhow::Result<()> {
    let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let token_issuer = authorization_server.map(|value| value.issuer.clone());
    let token_subject = client_id
        .map(|value| TokenSubject::new(value.as_str()))
        .transpose()?;
    let principal = match (authorization_server, client_id) {
        (Some(authorization_server), Some(client_id)) => Some(PrincipalId::new(format!(
            "{}#{}",
            authorization_server.issuer, client_id
        ))?),
        _ => None,
    };
    let latency_ms = u64::try_from(started_at.elapsed().as_millis())?;
    gateway_state.record_auth_audit_event(&AuthAuditEvent {
        event_id,
        timestamp: chrono::Utc::now(),
        trace_id,
        profile: profile.id.clone(),
        protected_resource: profile.protected_resource.clone(),
        outcome,
        reason,
        method: AuthMethod::ClientCredentialsPrivateKeyJwt,
        principal,
        tenant: None,
        token_issuer,
        token_subject,
        jwt_id: jwt_id.cloned(),
        latency_ms: Some(latency_ms),
        metadata: Default::default(),
    })
}

fn record_id_jag_auth_audit(
    gateway_state: &GatewayState,
    profile: &GatewayProfile,
    authorization_server: Option<&ResourceAuthorizationServer>,
    client_id: Option<&OAuthClientId>,
    principal: Option<&Principal>,
    jwt_id: Option<&JwtId>,
    outcome: AuthOutcome,
    reason: AuthReasonCode,
    started_at: Instant,
) -> anyhow::Result<()> {
    let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())?;
    let token_issuer = authorization_server.map(|value| value.issuer.clone());
    let token_subject = match (principal, client_id) {
        (Some(principal), _) => Some(principal.subject.clone()),
        (None, Some(client_id)) => Some(TokenSubject::new(client_id.as_str())?),
        (None, None) => None,
    };
    let principal_id = match (principal, authorization_server, client_id) {
        (Some(principal), _, _) => Some(principal.id.clone()),
        (None, Some(authorization_server), Some(client_id)) => Some(PrincipalId::new(format!(
            "{}#{}",
            authorization_server.issuer, client_id
        ))?),
        _ => None,
    };
    let tenant = principal.and_then(|value| value.tenant.clone());
    let latency_ms = u64::try_from(started_at.elapsed().as_millis())?;
    gateway_state.record_auth_audit_event(&AuthAuditEvent {
        event_id,
        timestamp: chrono::Utc::now(),
        trace_id,
        profile: profile.id.clone(),
        protected_resource: profile.protected_resource.clone(),
        outcome,
        reason,
        method: AuthMethod::EnterpriseManagedIdJag,
        principal: principal_id,
        tenant,
        token_issuer,
        token_subject,
        jwt_id: jwt_id.cloned(),
        latency_ms: Some(latency_ms),
        metadata: Default::default(),
    })
}

fn auth_audit_error_response(err: anyhow::Error) -> axum::response::Response {
    tracing::error!("failed to record gateway auth audit event: {err}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

fn internal_error_response(err: impl std::fmt::Display) -> axum::response::Response {
    tracing::error!("gateway internal error: {err}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

fn unauthorized(state: &ProfileAuthState, reason: &'static str) -> axum::response::Response {
    let catalog = current_catalog(&state.catalog);
    let Some(profile) = catalog.profile(&state.profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let metadata_url = format!(
        "{}/.well-known/oauth-protected-resource/mcp/{}",
        state.public_base_url, profile.id
    );
    let challenge = www_authenticate_challenge(&metadata_url, &profile.required_scopes);
    let Ok(challenge) = HeaderValue::from_str(&challenge) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut headers = HeaderMap::new();
    headers.insert(WWW_AUTHENTICATE, challenge);
    tracing::debug!(profile = %state.profile_id, reason, "gateway authorization challenge");
    (
        StatusCode::UNAUTHORIZED,
        headers,
        "authorization required for gateway profile",
    )
        .into_response()
}

fn allowed_gateway_jwt_algorithms() -> Vec<Algorithm> {
    vec![
        Algorithm::RS256,
        Algorithm::RS384,
        Algorithm::RS512,
        Algorithm::PS256,
        Algorithm::PS384,
        Algorithm::PS512,
        Algorithm::ES256,
        Algorithm::ES384,
        Algorithm::EdDSA,
    ]
}
