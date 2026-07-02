use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path as AxumPath, Request, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE},
    },
    middleware::{self, Next},
    response::IntoResponse,
    routing::get,
};
use clap::{Parser, Subcommand};
use jsonwebtoken::{Algorithm, jwk::JwkSet};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayProfileId, PublicDeployment};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, BearerToken, GatewayCatalog, GatewayMcp, JwtAuthConfig, JwtVerifier,
    www_authenticate_challenge,
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
    },
}

#[derive(Clone)]
struct AppState {
    catalog: Arc<GatewayCatalog>,
}

#[derive(Clone)]
struct ProfileAuthState {
    catalog: Arc<GatewayCatalog>,
    profile_id: GatewayProfileId,
    public_base_url: String,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct Readiness {
    status: &'static str,
    servers: usize,
    profiles: usize,
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
        Command::Serve {
            port,
            public_base_url,
            control_plane,
        } => serve(port, public_base_url, control_plane).await,
    }
}

async fn serve(port: u16, public_base_url: String, control_plane: PathBuf) -> anyhow::Result<()> {
    let catalog = Arc::new(GatewayCatalog::load_json(&control_plane)?);
    let deployment = PublicDeployment::new(public_base_url)?;
    let ct = CancellationToken::new();
    let allowed_hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
        deployment.host_authority().to_string(),
    ];
    let http = reqwest::Client::new();
    let state = AppState {
        catalog: catalog.clone(),
    };

    let mut router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route(
            "/.well-known/oauth-protected-resource/mcp/{profile}",
            get(protected_resource_metadata),
        )
        .with_state(state);

    for profile in catalog.profiles() {
        let profile_id = profile.id.clone();
        let mcp_service = StreamableHttpService::new(
            {
                let catalog = catalog.clone();
                let profile_id = profile_id.clone();
                move || Ok(GatewayMcp::new(catalog.clone(), profile_id.clone()))
            },
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default()
                .with_allowed_hosts(allowed_hosts.clone())
                .with_cancellation_token(ct.child_token()),
        );
        let auth_state = ProfileAuthState {
            catalog: catalog.clone(),
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
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(
        "veoveo-mcp-gateway listening on http://{addr} with {} server(s), {} profile(s)",
        catalog.server_count(),
        catalog.profile_count()
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
    Json(Readiness {
        status: "ready",
        servers: state.catalog.server_count(),
        profiles: state.catalog.profile_count(),
    })
}

async fn protected_resource_metadata(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
) -> impl IntoResponse {
    let Ok(profile_id) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match state.catalog.protected_resource_metadata(&profile_id) {
        Ok(metadata) => {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, Json(metadata)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn authenticate_mcp(
    State(state): State<ProfileAuthState>,
    mut request: Request,
    next: Next,
) -> axum::response::Response {
    let Some(profile) = state.catalog.profile(&state.profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(identity_provider) = state.catalog.identity_provider(&profile.identity_provider)
    else {
        return unauthorized(&state, "unknown identity provider");
    };

    let Some(header) = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return unauthorized(&state, "missing authorization header");
    };
    let token = match BearerToken::from_authorization_header(header) {
        Ok(token) => token,
        Err(err) => {
            tracing::warn!("rejected gateway request: {err}");
            return unauthorized(&state, "invalid authorization header");
        }
    };

    let jwks = match fetch_jwks(&state.http, identity_provider.jwks_uri.as_str()).await {
        Ok(jwks) => jwks,
        Err(err) => {
            tracing::warn!("failed to fetch identity provider JWKS: {err}");
            return unauthorized(&state, "identity provider unavailable");
        }
    };
    let auth_config = match JwtAuthConfig::new(
        identity_provider.issuer.clone(),
        profile.protected_resource.clone(),
        profile.required_scopes.iter().cloned().collect(),
        allowed_gateway_jwt_algorithms(),
    ) {
        Ok(config) => config,
        Err(err) => {
            tracing::error!("invalid gateway auth config: {err}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let subject = match JwtVerifier::new(auth_config, jwks).verify(&token) {
        Ok(subject) => subject,
        Err(err) => {
            tracing::warn!("rejected gateway token: {err}");
            return unauthorized(&state, "invalid bearer token");
        }
    };

    request
        .extensions_mut()
        .insert::<AuthenticatedSubject>(subject);
    next.run(request).await
}

async fn fetch_jwks(http: &reqwest::Client, url: &str) -> anyhow::Result<JwkSet> {
    let response = http.get(url).send().await?.error_for_status()?;
    Ok(response.json::<JwkSet>().await?)
}

fn unauthorized(state: &ProfileAuthState, reason: &'static str) -> axum::response::Response {
    let Some(profile) = state.catalog.profile(&state.profile_id) else {
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
