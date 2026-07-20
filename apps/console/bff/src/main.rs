mod api;
mod apps;
mod cluster;
mod config;
mod mcp_client;
mod oauth;
mod recording_playback;
mod session;

use std::{sync::Arc, time::Duration};

use anyhow::Context;
use axum::{
    Router, middleware,
    response::Redirect,
    routing::{delete, get, post, put},
};
use config::Config;
use session::SessionCipher;
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use veoveo_mcp_contract::{TelemetryGuard, init_server_telemetry};

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    http: reqwest::Client,
    stream_http: reqwest::Client,
    live_http: reqwest::Client,
    cluster: Option<Arc<cluster::KubernetesClient>>,
    sessions: SessionCipher,
    playback_tickets: recording_playback::PlaybackTicketStore,
    mcp: Arc<mcp_client::McpSessionPool>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-console-bff", "info,veoveo_console_bff=debug")?;
    let config = Arc::new(Config::from_env()?);
    let sessions = SessionCipher::new(config.session_key())?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .context("building console HTTP client")?;
    let stream_http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("building console streaming HTTP client")?;
    let live_http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("building console live HTTP client")?;
    let cluster = cluster::KubernetesClient::from_env()?.map(Arc::new);
    let mcp = Arc::new(mcp_client::McpSessionPool::new()?);
    let state = AppState {
        config: config.clone(),
        http,
        stream_http,
        live_http,
        cluster,
        sessions,
        playback_tickets: recording_playback::PlaybackTicketStore::default(),
        mcp,
    };
    let csrf_state = state.clone();

    let assets = ServeDir::new(config.asset_dir())
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(config.asset_dir().join("index.html")));
    let router = Router::new()
        .route("/", get(|| async { Redirect::permanent("/console/") }))
        .route("/healthz", get(|| async { "ok" }))
        .route("/auth/login", get(oauth::login))
        .route("/auth/callback", get(oauth::callback))
        .route("/auth/logout", post(oauth::logout))
        .route("/console/api/snapshot", get(api::snapshot))
        .route("/console/api/stream", get(api::stream))
        .route("/console/api/apps", get(apps::list_apps))
        .route("/console/api/apps/frame", get(apps::app_frame))
        .route("/console/api/apps/call", post(apps::call_app_tool))
        .route("/console/api/apps/read", post(apps::read_app_resource))
        .route("/console/api/cluster", get(cluster::snapshot))
        .route(
            "/console/api/tasks/{task_id}/cancel",
            post(api::cancel_task),
        )
        .route(
            "/console/api/artifacts/{artifact_id}/release-state",
            put(api::set_artifact_release_state),
        )
        .route(
            "/console/api/artifacts/{artifact_id}/grants",
            post(api::grant_artifact).delete(api::revoke_artifact_grant),
        )
        .route(
            "/console/api/artifacts/{artifact_id}/share-links",
            post(api::create_artifact_share_link),
        )
        .route(
            "/console/api/artifacts/{artifact_id}/share-links/{link_id}",
            delete(api::revoke_artifact_share_link),
        )
        .route(
            "/console/api/artifacts/{artifact_id}/access-requests",
            post(api::create_artifact_access_request),
        )
        .route(
            "/console/api/artifact-access-requests",
            get(api::list_artifact_access_requests),
        )
        .route(
            "/console/api/artifact-access-requests/{request_id}/decision",
            post(api::decide_artifact_access_request),
        )
        .route(
            "/console/api/artifact-access-requests/{request_id}/cancel",
            post(api::cancel_artifact_access_request),
        )
        .route(
            "/console/api/artifacts/{artifact_id}/download",
            get(api::download_artifact),
        )
        .route(
            "/console/api/artifacts/{artifact_id}/preview",
            get(api::preview_artifact),
        )
        .route(
            "/console/api/recordings/{recording_id}/playback",
            get(recording_playback::manifest),
        )
        .route(
            "/console/api/recordings/{recording_id}/sources/{ticket}/segments/{segment_id}/data.rrd",
            get(recording_playback::segment),
        )
        .route(
            "/console/api/recordings/{recording_id}/sources/{ticket}/segments/{segment_id}/live.rrd",
            get(recording_playback::live_segment),
        )
        .nest_service("/console", assets)
        .fallback(get(|| async { axum::http::StatusCode::NOT_FOUND }))
        .with_state(state)
        .layer(middleware::from_fn_with_state(csrf_state, api::enforce_csrf))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            axum::http::HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::REFERRER_POLICY,
            axum::http::HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::HeaderName::from_static("content-security-policy"),
            axum::http::HeaderValue::from_static(
                "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; connect-src 'self'; frame-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
            ),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_FRAME_OPTIONS,
            axum::http::HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::HeaderName::from_static("permissions-policy"),
            axum::http::HeaderValue::from_static(
                "camera=(), microphone=(), geolocation=(), payment=()",
            ),
        ))
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.bind())
        .await
        .with_context(|| format!("binding console BFF to {}", config.bind()))?;
    tracing::info!(address = %config.bind(), "console BFF listening");
    axum::serve(listener, router).await?;
    Ok(())
}
