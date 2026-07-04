use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{any, get, post},
};
use parking_lot::RwLock;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenIssuer, GatewayProfileId,
    InternalTokenSecret, PublicDeployment, TokenIssuer, public_allowed_hosts,
};
use veoveo_mcp_gateway::{GatewayCatalog, GatewayCatalogHandle, GatewayMcp};

use super::{
    admin::{
        prune_jwt_revocations, read_control_plane, reload_control_plane, revoke_jwt,
        update_control_plane,
    },
    auth::{
        authenticate_mcp, authorization_server_jwks, authorization_server_metadata,
        protected_resource_metadata,
    },
    host::validate_host,
    oauth::{authorization_callback, authorize_endpoint, token_endpoint},
    runtime::{
        AdminState, AppState, DynamicMcpState, GatewayRetentionPolicy, ProfileAuthState,
        ProfileMcpService, Readiness, RuntimeControlPlaneSource, build_http_client,
        current_catalog, profile_id_from_gateway_path, run_gateway_retention_gc,
        spawn_gateway_retention_gc_loop,
    },
};

pub(super) async fn serve(
    port: u16,
    public_base_url: String,
    control_plane: RuntimeControlPlaneSource,
    state_db: PathBuf,
    internal_token_secret: String,
    allow_loopback_hosts: bool,
    retention: GatewayRetentionPolicy,
) -> anyhow::Result<()> {
    let gateway_state = veoveo_mcp_gateway::GatewayState::open(&state_db)?;
    run_gateway_retention_gc(&gateway_state, retention)?;
    spawn_gateway_retention_gc_loop(gateway_state.clone(), retention);
    let initial_catalog = load_initial_catalog(&control_plane).await?;
    let catalog = GatewayCatalogHandle::new(initial_catalog.clone());
    let internal_token_issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        InternalTokenSecret::new(internal_token_secret)?,
    );
    let deployment = PublicDeployment::new(public_base_url)?;
    let ct = CancellationToken::new();
    let allowed_hosts = Arc::new(public_allowed_hosts(&deployment, allow_loopback_hosts));
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
        control_plane,
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
    let router = router
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
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

async fn load_initial_catalog(
    source: &RuntimeControlPlaneSource,
) -> anyhow::Result<Arc<GatewayCatalog>> {
    match source {
        RuntimeControlPlaneSource::File { path } => {
            let catalog = Arc::new(GatewayCatalog::load_json(path)?);
            tracing::info!(
                path = %path.display(),
                server_count = catalog.server_count(),
                profile_count = catalog.profile_count(),
                "loaded mounted gateway control plane"
            );
            Ok(catalog)
        }
        RuntimeControlPlaneSource::Postgres { db } => {
            db.migrate().await?;
            let revision = db
                .load_active_revision()
                .await?
                .context("gateway control-plane Postgres has no active revision; seed it first")?;
            let catalog = Arc::new(GatewayCatalog::from_control_plane(revision.control_plane)?);
            tracing::info!(
                revision_id = %revision.revision_id,
                sha256 = %revision.sha256,
                source = ?revision.source,
                server_count = catalog.server_count(),
                profile_count = catalog.profile_count(),
                "loaded active gateway control-plane revision from Postgres"
            );
            Ok(catalog)
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_allowed_hosts_use_public_authority_only() {
        let deployment = PublicDeployment::new("https://veoveo.bioma.ai").expect("valid URL");

        assert_eq!(
            public_allowed_hosts(&deployment, false),
            vec!["veoveo.bioma.ai"]
        );
    }

    #[test]
    fn local_allowed_hosts_are_explicit() {
        let deployment = PublicDeployment::new("https://veoveo.bioma.ai").expect("valid URL");

        assert_eq!(
            public_allowed_hosts(&deployment, true),
            vec!["veoveo.bioma.ai", "localhost", "127.0.0.1", "::1"]
        );
    }
}
