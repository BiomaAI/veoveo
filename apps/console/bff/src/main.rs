mod api;
mod config;
mod oauth;
mod session;

use std::sync::Arc;

use anyhow::Context;
use axum::{
    Router, middleware,
    routing::{any, delete, get, post, put},
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
    sessions: SessionCipher,
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
        .build()
        .context("building console HTTP client")?;
    let state = AppState {
        config: config.clone(),
        http,
        sessions,
    };
    let csrf_state = state.clone();

    let assets = ServeDir::new(config.asset_dir())
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(config.asset_dir().join("index.html")));
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/auth/login", get(oauth::login))
        .route("/auth/callback", get(oauth::callback))
        .route("/auth/logout", post(oauth::logout))
        .route("/console/api/snapshot", get(api::snapshot))
        .route("/console/api/map/{*path}", any(api::map_admin))
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
            "/console/api/artifacts/{artifact_id}/download",
            get(api::download_artifact),
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
                "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
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
