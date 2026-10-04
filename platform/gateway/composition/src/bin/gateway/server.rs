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
        create_artifact_access_request, create_artifact_share_link, decide_artifact_access_request,
        grant_artifact, list_artifact_access_requests, proxy_server_admin, prune_jwt_revocations,
        read_console_artifact, read_console_snapshot, read_control_plane, read_server_health,
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
    runtime::{
        AdminState, AppState, ArtifactHttpState, DynamicMcpState, ProfileAuthState,
        ProfileMcpService, Readiness, build_http_client, current_catalog,
        spawn_authorization_retention_gc_loop, spawn_refresh_delivery_gc_loop,
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
    pub(super) audit_retention_days: std::num::NonZeroU32,
    pub(super) audit_exports: veoveo_audit::export::AuditExportConfig,
    pub(super) audit_signing_key: Arc<veoveo_audit::integrity::AuditSigningKey>,
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
        audit_retention_days,
        audit_signing_key,
        audit_exports,
    } = config;
    let initial_catalog =
        load_initial_catalog(&control_store, expected_control_plane_sha256.as_deref()).await?;
    let managed_templates = Arc::new(
        veoveo_agent_runtime::gateway::ManagedTemplateCatalog::from_env(&initial_catalog)?,
    );
    let gateway_state = crate::bindings::gateway_state(
        control_store.platform_store().clone(),
        managed_templates.clone(),
    )?;
    let audit_store = control_store.platform_store().clone();
    let catalog = GatewayCatalogHandle::new(initial_catalog.clone());
    let internal_signing_key_der = BASE64_STANDARD
        .decode(internal_signing_key_der_b64.expose_secret().trim())
        .context("internal signing key must be base64-encoded Ed25519 PKCS#8 DER")?;
    let internal_token_issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(internal_signing_key_id, internal_signing_key_der)?,
    );
    let deployment = PublicDeployment::new(public_base_url)?;
    let ct = CancellationToken::new();
    let _stop = ct.clone().drop_guard();
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
        deployment: deployment.clone(),
        auth_http: http.clone(),
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

    let artifact_http_state = ArtifactHttpState {
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
        http: reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .read_timeout(std::time::Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        internal_token_issuer: internal_token_issuer.clone(),
        artifact_server: veoveo_mcp_contract::ServerSlug::parse("artifact")?,
        artifact_service_url: artifact_service_url.trim_end_matches('/').to_owned(),
    };
    let artifact_download_router = Router::new()
        .route(
            "/artifacts/{profile}/{artifact_id}/download",
            get(download_artifact),
        )
        .with_state(artifact_http_state.clone())
        .merge(crate::artifact_upload::router(artifact_http_state))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_mcp,
        ));
    router = router.merge(artifact_download_router);

    router = router.merge(
        Router::new()
            .route(
                "/console-api/{profile}/session",
                get(crate::console::bootstrap),
            )
            .with_state(crate::console::ConsoleState {
                catalog: catalog.clone(),
                offline_mode,
            })
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                authenticate_mcp,
            )),
    );

    let context = veoveo_mcp_gateway::http::GatewayHttpContext {
        deployment: deployment.clone(),
        catalog: catalog.clone(),
        gateway_state: gateway_state.clone(),
        internal_token_issuer: internal_token_issuer.clone(),
        upstream_http: upstream_http.clone(),
        auth_http: http.clone(),
    };
    let mut registrations = veoveo_mcp_gateway::http::GatewayModules::new();
    let mut required = vec![];
    for (name, factory) in [
        (
            "computers",
            veoveo_computers::gateway::routes::build as fn(_, _) -> _,
        ),
        ("speech", veoveo_speech_mcp::gateway::build as fn(_, _) -> _),
    ] {
        let module = veoveo_modules::ModuleName::new(name)?;
        required.push(module.clone());
        registrations.register(module, move |context, scope| {
            Box::pin(async move { factory(context, scope) })
        })?;
    }
    let recording_url = artifact_service_url.trim_end_matches('/').to_owned();
    let recordings = veoveo_modules::ModuleName::new("recordings")?;
    required.push(recordings.clone());
    registrations.register(recordings, move |context, scope| {
        Box::pin(async move {
            veoveo_recording_mcp::gateway::routes::build(context, scope, recording_url)
        })
    })?;
    super::owner_modules::register(
        &mut registrations,
        &mut required,
        std::num::NonZeroU16::new(port).context("gateway port must be positive")?,
        &context,
        managed_templates,
    )?;
    let _module_cleanup = registrations.supervisor();
    let modules = registrations.build(context, &required).await?;
    let module_shutdown = modules.shutdown_signal();
    let module_bindings = Arc::new(modules.bindings().to_vec());
    for binding in module_bindings.iter() {
        tracing::info!(module=%binding.module,state=?binding.state,required=binding.required,"gateway module binding");
    }
    let module_router = modules.router();
    modules.run(async {
    let addr=SocketAddr::from(([0,0,0,0],port));
    let listener=tokio::net::TcpListener::bind(addr).await?;
    router = router.merge(module_router);


    let server_health =
        spawn_server_health_prober(catalog.clone(), upstream_http.clone(), ct.child_token());
    let console_stream =
        spawn_console_wake_hub(control_store.platform_store().clone(), ct.child_token());
    let admin_state = AdminState {
        catalog: catalog.clone(),
        http: http.clone(),
        control_store,
        gateway_state: gateway_state.clone(),
        internal_token_issuer,
        upstream_http,
        artifact_server: veoveo_mcp_contract::ServerSlug::parse("artifact")?,
        artifact_service_url,
        offline_mode,
        server_health,
        module_bindings,
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
            "/admin/{profile}/console/artifacts/{artifact_id}",
            get(read_console_artifact),
        )
        .route(
            "/admin/{profile}/console/cluster",
            get(authorize_console_cluster),
        )
        .route("/admin/{profile}/console/stream", get(stream_console))
        .route(
            "/admin/{profile}/console/audit/views",
            post(super::admin::console_audit::open_view),
        )
        .route(
            "/admin/{profile}/console/audit/partitions",
            get(super::admin::console_audit::partitions),
        )
        .route(
            "/admin/{profile}/console/audit/records",
            get(super::admin::console_audit::records),
        )
        .route(
            "/admin/{profile}/console/audit/summary",
            get(super::admin::console_audit::summary),
        )
        .route(
            "/admin/{profile}/console/audit/stream",
            get(super::admin::console_audit::stream_audit),
        )
        .route(
            "/admin/{profile}/console/audit/export",
            get(super::admin::console_audit::export_audit),
        )
        .route("/admin/{profile}/jwt-revocations", post(revoke_jwt))
        .route(
            "/admin/{profile}/jwt-revocations/prune",
            post(prune_jwt_revocations),
        )
        .route(
            "/admin/{profile}/tasks/{server}/{task_id}/cancel",
            post(cancel_task),
        )
        .route("/admin/{profile}/server-health", get(read_server_health))
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
        .layer(middleware::from_fn(
            veoveo_mcp_gateway::request_observation::observe_request,
        ))
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    tracing::info!(
        service = "veoveo-mcp-gateway",
        address = %addr,
        server_count = initial_catalog.server_count(),
        profile_count = initial_catalog.profile_count(),
        "listening"
    );
    let audit = gateway_state.audit_writer().await.clone();
    let audit_service = veoveo_audit::AuditService::start(
        audit_store,
        audit_signing_key,
        audit_retention_days,
        audit_exports,
    )?;
    gateway_state.set_audit_health(audit_service.health())?;
    let authorization_gc =
        spawn_authorization_retention_gc_loop(gateway_state.clone(), ct.child_token());
    let refresh_gc = spawn_refresh_delivery_gc_loop(gateway_state.clone(), ct.child_token());
    let serving = std::future::IntoFuture::into_future(axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown({
        let audit = audit.clone();
        let ct = ct.clone();
        let module_shutdown=module_shutdown.clone();
        async move {
            let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {}, _ = audit.closed() => {} }
            module_shutdown.cancel();
            ct.cancel();
        }
    }));
    tokio::pin!(serving);
    let result: anyhow::Result<()> = tokio::select! {
        result = &mut serving => result.map_err(Into::into),
        _ = ct.cancelled() => tokio::time::timeout(std::time::Duration::from_secs(30), &mut serving)
            .await.context("gateway HTTP shutdown deadline exceeded").and_then(|result| result.map_err(Into::into)),
    };
    module_shutdown.cancel();
    ct.cancel();
    let modules_drained = module_shutdown.drain().await;
    let drained = audit.shutdown(std::time::Duration::from_secs(30)).await;
    let sealed = audit_service
        .shutdown(std::time::Duration::from_secs(30))
        .await;
    let cleanup = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        authorization_gc.await?;
        refresh_gc.await?;
        Ok::<(), tokio::task::JoinError>(())
    })
    .await;
    result?;
    modules_drained?;
    drained?;
    sealed?;
    cleanup??;
    Ok(())
    }).await
}

async fn load_initial_catalog(
    store: &GatewayControlStore,
    expected_sha256: Option<&str>,
) -> anyhow::Result<Arc<GatewayCatalog>> {
    let revision = store.load_active_revision().await?.context(
        "SurrealDB platform store has no active gateway control-plane revision; run control-plane-publish after installation preparation and module lanes",
    )?;
    verify_expected_control_plane_revision(&revision.sha256, expected_sha256)?;
    let catalog = Arc::new(GatewayCatalog::from_control_plane(
        revision.control_plane,
        store.admission(),
    )?);
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
            "active gateway control-plane revision {active_sha256} does not match requested revision {expected_sha256}; control-plane-publish has not converged"
        );
    }
    Ok(())
}

async fn dynamic_mcp_profile(
    State(state): State<DynamicMcpState>,
    request: Request,
) -> axum::response::Response {
    let Some(profile_id) = veoveo_mcp_gateway::http::profile_from_request(&request) else {
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
        .layer(middleware::from_fn(
            veoveo_mcp_gateway::mcp::http_response::preserve_upstream_http_rejection,
        ))
}

async fn readyz(State(state): State<AppState>) -> (StatusCode, Json<Readiness>) {
    let catalog = current_catalog(&state.catalog);
    let ready = state.gateway_state.audit_ready();
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(Readiness {
            status: if ready { "ready" } else { "audit_unavailable" },
            servers: catalog.server_count(),
            profiles: catalog.profile_count(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_routes_enter_the_same_profile_authentication_boundary() {
        for path in [
            "/speech/workspace/dictation",
            "/speech/workspace/dictation/123/chunks/0",
            "/speech/workspace/dictation/123/finish",
        ] {
            assert_eq!(
                veoveo_mcp_gateway::http::profile_from_route("/speech/{profile}/{*path}", path)
                    .unwrap()
                    .as_str(),
                "workspace"
            );
        }
        assert!(
            veoveo_mcp_gateway::http::profile_from_route(
                "/speech/{profile}/dictation",
                "/speech//dictation"
            )
            .is_none()
        );
    }

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
            veoveo_mcp_gateway::http::profile_from_route(
                "/artifacts/{profile}/{artifact_id}/download",
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
            veoveo_mcp_gateway::http::profile_from_route(
                "/recordings/{profile}/{recording_id}/live/rrd-stream",
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
                .contains("control-plane-publish has not converged")
        );
        verify_expected_control_plane_revision("requested", Some("requested")).unwrap();
        verify_expected_control_plane_revision("old", None).unwrap();
    }
}
