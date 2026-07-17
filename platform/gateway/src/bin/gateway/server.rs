use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Redirect},
    routing::{any, get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use parking_lot::RwLock;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use secrecy::{ExposeSecret, SecretString};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalSigningKey, GatewayInternalTokenIssuer,
    GatewayProfileId, PublicDeployment, TokenIssuer, public_allowed_hosts,
};
use veoveo_mcp_gateway::{
    GatewayCatalog, GatewayCatalogHandle, GatewayControlStore, GatewayMcp,
    GatewayRefreshDeliveryWindow, GatewayTaskExtension, RefreshTokenDeliveryCipher,
};

use super::{
    admin::{
        authorize_console_cluster, cancel_task, create_artifact_share_link, grant_artifact,
        proxy_server_admin, prune_jwt_revocations, read_console_snapshot, read_control_plane,
        revoke_artifact_grant, revoke_artifact_share_link, revoke_jwt, set_artifact_release_state,
        spawn_console_wake_hub, spawn_server_health_prober, stream_console, update_control_plane,
    },
    artifact_download::download_artifact,
    auth::{
        authenticate_mcp, authorization_server_jwks, authorization_server_metadata,
        protected_resource_metadata,
    },
    host::validate_host,
    oauth::{authorization_callback, authorize_endpoint, revoke_refresh_token, token_endpoint},
    recording_ingest::recording_ingest_router,
    recording_playback::{playback_manifest, playback_segment},
    runtime::{
        AdminState, AppState, ArtifactDownloadState, DynamicMcpState, GatewayRetentionPolicy,
        ProfileAuthState, ProfileMcpService, Readiness, RecordingIngestGatewayState,
        RecordingPlaybackState, build_http_client, current_catalog, profile_id_from_gateway_path,
        run_gateway_retention_gc, spawn_gateway_retention_gc_loop, spawn_refresh_delivery_gc_loop,
    },
};

pub(super) struct ServeConfig {
    pub(super) port: u16,
    pub(super) public_base_url: String,
    pub(super) artifact_service_url: String,
    pub(super) control_store: GatewayControlStore,
    pub(super) internal_signing_key_der_b64: SecretString,
    pub(super) internal_signing_key_id: String,
    pub(super) refresh_delivery_cipher: RefreshTokenDeliveryCipher,
    pub(super) refresh_delivery_window: GatewayRefreshDeliveryWindow,
    pub(super) allow_loopback_hosts: bool,
    pub(super) offline_mode: bool,
    pub(super) retention: GatewayRetentionPolicy,
}

pub(super) async fn serve(config: ServeConfig) -> anyhow::Result<()> {
    let ServeConfig {
        port,
        public_base_url,
        artifact_service_url,
        control_store,
        internal_signing_key_der_b64,
        internal_signing_key_id,
        refresh_delivery_cipher,
        refresh_delivery_window,
        allow_loopback_hosts,
        offline_mode,
        retention,
    } = config;
    let gateway_state =
        veoveo_mcp_gateway::GatewayState::new(control_store.platform_store().clone());
    run_gateway_retention_gc(&gateway_state, retention).await?;
    spawn_gateway_retention_gc_loop(gateway_state.clone(), retention);
    spawn_refresh_delivery_gc_loop(gateway_state.clone());
    let initial_catalog = load_initial_catalog(&control_store).await?;
    let catalog = GatewayCatalogHandle::new(initial_catalog.clone());
    let internal_signing_key_der = BASE64_STANDARD
        .decode(internal_signing_key_der_b64.expose_secret().trim())
        .context("internal signing key must be base64-encoded Ed25519 PKCS#8 DER")?;
    let internal_token_issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(internal_signing_key_id, internal_signing_key_der)?,
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
        refresh_delivery_cipher,
        refresh_delivery_window,
    };

    let mut router = Router::new()
        .route("/", get(|| async { Redirect::permanent("/console/") }))
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route("/oauth/authorize", get(authorize_endpoint))
        .route("/oauth/callback", get(authorization_callback))
        .route("/oauth/token", post(token_endpoint))
        .route("/oauth/revoke", post(revoke_refresh_token))
        .route(
            "/.well-known/oauth-protected-resource/mcp/{profile}",
            get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/oauth",
            get(authorization_server_metadata),
        )
        .route("/oauth/jwks.json", get(authorization_server_jwks))
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
        platform_store: control_store.platform_store().clone(),
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

    router = router.merge(recording_ingest_router(RecordingIngestGatewayState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
        http: http.clone(),
        internal_token_issuer: internal_token_issuer.clone(),
        public_base_url: deployment.base_url().to_string(),
    }));

    let artifact_download_state = ArtifactDownloadState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
        http: http.clone(),
        internal_token_issuer: internal_token_issuer.clone(),
        artifact_server: veoveo_mcp_contract::ServerSlug::new("artifact")?,
        artifact_service_url: artifact_service_url.trim_end_matches('/').to_owned(),
    };
    let artifact_download_router = Router::new()
        .route(
            "/artifacts/{profile}/{artifact_id}/download",
            get(download_artifact),
        )
        .with_state(artifact_download_state)
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_mcp,
        ));
    router = router.merge(artifact_download_router);

    let recording_playback_router = Router::new()
        .route(
            "/recordings/{profile}/{recording_id}/playback",
            get(playback_manifest),
        )
        .route(
            "/recordings/{profile}/{recording_id}/segments/{segment_id}",
            get(playback_segment),
        )
        .with_state(RecordingPlaybackState {
            catalog: catalog.clone(),
            gateway_state: gateway_state.clone(),
            internal_token_issuer: internal_token_issuer.clone(),
        })
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_mcp,
        ));
    router = router.merge(recording_playback_router);

    let server_health = spawn_server_health_prober(catalog.clone(), ct.child_token());
    let console_stream =
        spawn_console_wake_hub(control_store.platform_store().clone(), ct.child_token());
    let admin_state = AdminState {
        catalog: catalog.clone(),
        http: http.clone(),
        control_store,
        gateway_state: gateway_state.clone(),
        internal_token_issuer,
        artifact_server: veoveo_mcp_contract::ServerSlug::new("artifact")?,
        artifact_service_url,
        offline_mode,
        server_health,
        console_stream,
    };
    let admin_router = Router::new()
        .route(
            "/admin/{profile}/control-plane",
            get(read_control_plane).put(update_control_plane),
        )
        .route(
            "/admin/{profile}/console/snapshot",
            get(read_console_snapshot),
        )
        .route(
            "/admin/{profile}/console/cluster",
            get(authorize_console_cluster),
        )
        .route("/admin/{profile}/console/stream", get(stream_console))
        .route("/admin/{profile}/jwt-revocations", post(revoke_jwt))
        .route(
            "/admin/{profile}/jwt-revocations/prune",
            post(prune_jwt_revocations),
        )
        .route("/admin/{profile}/tasks/{task_id}/cancel", post(cancel_task))
        .route(
            "/admin/{profile}/servers/{server}/{*path}",
            any(proxy_server_admin),
        )
        .route(
            "/admin/{profile}/artifacts/{artifact_id}/release-state",
            axum::routing::put(set_artifact_release_state),
        )
        .route(
            "/admin/{profile}/artifacts/{artifact_id}/grants",
            post(grant_artifact).delete(revoke_artifact_grant),
        )
        .route(
            "/admin/{profile}/artifacts/{artifact_id}/share-links",
            post(create_artifact_share_link),
        )
        .route(
            "/admin/{profile}/artifacts/{artifact_id}/share-links/{link_id}",
            axum::routing::delete(revoke_artifact_share_link),
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

async fn load_initial_catalog(store: &GatewayControlStore) -> anyhow::Result<Arc<GatewayCatalog>> {
    let revision = store.load_active_revision().await?.context(
        "SurrealDB platform store has no active gateway control-plane revision; run installation-bootstrap first",
    )?;
    let catalog = Arc::new(GatewayCatalog::from_control_plane(revision.control_plane)?);
    tracing::info!(
        revision_id = %revision.revision_id,
        sha256 = %revision.sha256,
        source = ?revision.source,
        server_count = catalog.server_count(),
        profile_count = catalog.profile_count(),
        "loaded active gateway control-plane revision from the SurrealDB platform store"
    );
    Ok(catalog)
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
    service.oneshot(request).await.into_response()
}

fn build_profile_mcp_service(
    state: &DynamicMcpState,
    profile_id: GatewayProfileId,
) -> ProfileMcpService {
    let internal_token_issuer = state.internal_token_issuer.clone();
    let mcp_service = StreamableHttpService::new(
        {
            let catalog = state.catalog.clone();
            let gateway_state = state.gateway_state.clone();
            let platform_store = state.platform_store.clone();
            let profile_id = profile_id.clone();
            move || {
                Ok(GatewayMcp::new(
                    catalog.clone(),
                    profile_id.clone(),
                    gateway_state.clone(),
                    platform_store.clone(),
                    internal_token_issuer.clone(),
                ))
            }
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(state.allowed_hosts.iter().cloned())
            .with_cancellation_token(state.cancellation_token.child_token()),
    );
    let task_extension = Arc::new(veoveo_mcp_task_extension::TaskExtensionAdapter::new(
        Arc::new(GatewayTaskExtension::new(GatewayMcp::new(
            state.catalog.clone(),
            profile_id,
            state.gateway_state.clone(),
            state.platform_store.clone(),
            state.internal_token_issuer.clone(),
        ))),
        veoveo_mcp_task_extension::ServerDiscovery::new(
            BTreeMap::from([
                ("tools".to_owned(), serde_json::json!({})),
                ("resources".to_owned(), serde_json::json!({})),
            ]),
            veoveo_mcp_task_extension::Implementation {
                name: "veoveo-mcp-gateway".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some(
                "Profile-aware canonical task routing across governed Veoveo MCP servers."
                    .to_owned(),
            ),
        ),
    ));
    Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            veoveo_mcp_task_extension::task_extension_middleware::<GatewayTaskExtension>,
        ))
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
        let deployment = PublicDeployment::new("https://veoveo.example").expect("valid URL");

        assert_eq!(
            public_allowed_hosts(&deployment, false),
            vec!["veoveo.example"]
        );
    }

    #[test]
    fn local_allowed_hosts_are_explicit() {
        let deployment = PublicDeployment::new("https://veoveo.example").expect("valid URL");

        assert_eq!(
            public_allowed_hosts(&deployment, true),
            vec!["veoveo.example", "localhost", "127.0.0.1", "::1"]
        );
    }

    #[test]
    fn artifact_download_path_carries_the_authenticated_profile() {
        assert_eq!(
            profile_id_from_gateway_path(
                "/artifacts/operator/0197f78e-f2f0-7a6e-8a5d-f41c691e4471/download"
            )
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
            Some("operator")
        );
    }
}
