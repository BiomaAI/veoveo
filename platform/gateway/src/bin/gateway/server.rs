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
use rmcp::transport::streamable_http_server::StreamableHttpService;
use secrecy::{ExposeSecret, SecretString};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_agent_runtime::AgentControl;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalSigningKey, GatewayInternalTokenIssuer,
    GatewayProfileId, PublicDeployment, TokenIssuer, public_allowed_hosts,
};
use veoveo_mcp_gateway::{
    GatewayCatalog, GatewayCatalogHandle, GatewayControlStore, GatewayMcp,
    GatewayRefreshDeliveryWindow, GatewayUpstreamHttpClientPool, RefreshTokenDeliveryCipher,
};

use super::{
    admin::{
        authorize_console_cluster, cancel_artifact_access_request, cancel_task,
        create_artifact_access_request, create_artifact_share_link, decide_agent_input_request,
        decide_artifact_access_request, grant_artifact, list_agent_input_requests,
        list_artifact_access_requests, proxy_server_admin, prune_jwt_revocations,
        read_agent_conversation, read_console_snapshot, read_control_plane, revoke_artifact_grant,
        revoke_artifact_share_link, revoke_jwt, send_agent_message, set_artifact_release_state,
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
    recording_layer_publication::publish_recording_layer,
    recording_playback::{
        catalog_grant, playback_blueprint, playback_live_recording, playback_manifest,
        projection_data,
    },
    runtime::{
        AdminState, AppState, ArtifactDownloadState, DynamicMcpState, GatewayRetentionPolicy,
        ProfileAuthState, ProfileMcpService, Readiness, RecordingIngestGatewayState,
        RecordingLayerPublicationState, RecordingPlaybackState, build_http_client, current_catalog,
        profile_id_from_gateway_path, spawn_gateway_retention_gc_loop,
        spawn_refresh_delivery_gc_loop,
    },
};

pub(super) struct ServeConfig {
    pub(super) port: u16,
    pub(super) public_base_url: String,
    pub(super) artifact_service_url: String,
    pub(super) control_store: GatewayControlStore,
    pub(super) expected_control_plane_sha256: Option<String>,
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
        expected_control_plane_sha256,
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
    let agent_control = AgentControl::new(control_store.platform_store().clone())?;
    spawn_gateway_retention_gc_loop(gateway_state.clone(), retention);
    spawn_refresh_delivery_gc_loop(gateway_state.clone());
    let initial_catalog =
        load_initial_catalog(&control_store, expected_control_plane_sha256.as_deref()).await?;
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
    let upstream_http = GatewayUpstreamHttpClientPool::new();
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
        internal_token_issuer: internal_token_issuer.clone(),
        upstream_http: upstream_http.clone(),
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

    let recording_publication_router = Router::new()
        .route(
            "/recordings/{profile}/layers",
            axum::routing::post(publish_recording_layer),
        )
        .with_state(RecordingLayerPublicationState {
            catalog: catalog.clone(),
            gateway_state: gateway_state.clone(),
            http: http.clone(),
            internal_token_issuer: internal_token_issuer.clone(),
            artifact_server: veoveo_mcp_contract::ServerSlug::new("artifact")?,
            artifact_service_url: artifact_service_url.trim_end_matches('/').to_owned(),
        })
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_mcp,
        ));
    router = router.merge(recording_publication_router);

    let recording_playback_router = Router::new()
        .route("/recordings/{profile}/catalog-grants", post(catalog_grant))
        .route(
            "/recordings/{profile}/{recording_id}/playback",
            get(playback_manifest),
        )
        .route(
            "/recordings/{profile}/{recording_id}/live/rrd-stream",
            get(playback_live_recording),
        )
        .route(
            "/recordings/{profile}/{recording_id}/blueprints/{revision}/data.rrd",
            get(playback_blueprint),
        )
        .route(
            "/recordings/{profile}/{recording_id}/projections/{projection_id}/data.arrow",
            get(projection_data),
        )
        .with_state(RecordingPlaybackState {
            catalog: catalog.clone(),
            gateway_state: gateway_state.clone(),
            internal_token_issuer: internal_token_issuer.clone(),
            upstream_http: upstream_http.clone(),
            artifact_server: veoveo_mcp_contract::ServerSlug::new("artifact")?,
        })
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_mcp,
        ));
    router = router.merge(recording_playback_router);

    let server_health =
        spawn_server_health_prober(catalog.clone(), upstream_http.clone(), ct.child_token());
    let console_stream =
        spawn_console_wake_hub(control_store.platform_store().clone(), ct.child_token());
    let admin_state = AdminState {
        agent_control,
        catalog: catalog.clone(),
        http: http.clone(),
        control_store,
        gateway_state: gateway_state.clone(),
        internal_token_issuer,
        upstream_http,
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
            "/admin/{profile}/agents/{agent_id}/messages",
            post(send_agent_message),
        )
        .route(
            "/admin/{profile}/agents/{agent_id}/conversation",
            get(read_agent_conversation),
        )
        .route(
            "/admin/{profile}/agents/{agent_id}/input-requests",
            get(list_agent_input_requests),
        )
        .route(
            "/admin/{profile}/agents/{agent_id}/input-requests/{input_request_id}/decision",
            post(decide_agent_input_request),
        )
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
        .route(
            "/admin/{profile}/artifacts/{artifact_id}/access-requests",
            post(create_artifact_access_request),
        )
        .route(
            "/admin/{profile}/artifact-access-requests",
            get(list_artifact_access_requests),
        )
        .route(
            "/admin/{profile}/artifact-access-requests/{request_id}/decision",
            post(decide_artifact_access_request),
        )
        .route(
            "/admin/{profile}/artifact-access-requests/{request_id}/cancel",
            post(cancel_artifact_access_request),
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
    store: &GatewayControlStore,
    expected_sha256: Option<&str>,
) -> anyhow::Result<Arc<GatewayCatalog>> {
    let revision = store.load_active_revision().await?.context(
        "SurrealDB platform store has no active gateway control-plane revision; run installation-bootstrap first",
    )?;
    verify_expected_control_plane_revision(&revision.sha256, expected_sha256)?;
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

fn verify_expected_control_plane_revision(
    active_sha256: &str,
    expected_sha256: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(expected_sha256) = expected_sha256 {
        anyhow::ensure!(
            active_sha256 == expected_sha256,
            "active gateway control-plane revision {active_sha256} does not match requested revision {expected_sha256}; installation-bootstrap has not converged"
        );
    }
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
    service.oneshot(request).await.into_response()
}

fn build_profile_mcp_service(
    state: &DynamicMcpState,
    profile_id: GatewayProfileId,
) -> ProfileMcpService {
    // Every stateless request gets its own handler clone while the profile's
    // discovery cache and change broadcaster remain process-wide.
    let gateway_mcp = GatewayMcp::new(
        state.catalog.clone(),
        profile_id.clone(),
        state.gateway_state.clone(),
        state.internal_token_issuer.clone(),
        state.upstream_http.clone(),
    );
    let mcp_service = StreamableHttpService::new(
        move || Ok(gateway_mcp.clone()),
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(state.allowed_hosts.iter().cloned())
            .with_cancellation_token(state.cancellation_token.child_token()),
    );
    Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
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

    #[test]
    fn recording_live_stream_path_carries_the_authenticated_profile() {
        assert_eq!(
            profile_id_from_gateway_path(
                "/recordings/operator/019faa9f-acc8-7400-ba67-a9b022da1f63/live/rrd-stream"
            )
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
            Some("operator")
        );
    }

    #[test]
    fn requested_control_plane_revision_rejects_a_stale_active_revision() {
        let error = verify_expected_control_plane_revision("old", Some("requested"))
            .expect_err("stale active revision must fail closed");
        assert!(
            error
                .to_string()
                .contains("installation-bootstrap has not converged")
        );
        verify_expected_control_plane_revision("requested", Some("requested")).unwrap();
        verify_expected_control_plane_revision("old", None).unwrap();
    }
}
