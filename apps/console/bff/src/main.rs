mod api;
mod apps;
mod cluster;
mod config;
mod mcp_client;
mod oauth;
mod recording_playback;
mod session;

use std::{fs, path::Path, sync::Arc, time::Duration};

use anyhow::Context;
use axum::{
    Router,
    http::{HeaderValue, header::CACHE_CONTROL},
    middleware,
    response::{Html, Redirect},
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
    playback_sessions: recording_playback::PlaybackSessionStore,
    mcp: Arc<mcp_client::McpSessionPool>,
    app_tasks: apps::AppTaskRegistry,
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
        playback_sessions: recording_playback::PlaybackSessionStore::default(),
        mcp,
        app_tasks: apps::AppTaskRegistry::default(),
    };
    let csrf_state = state.clone();

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
        .route("/console/api/apps/task/get", post(apps::get_app_task))
        .route(
            "/console/api/apps/task/result",
            post(apps::get_app_task_result),
        )
        .route("/console/api/apps/task/cancel", post(apps::cancel_app_task))
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
            "/console/api/recordings/{recording_id}/playback-sessions/{playback_session}/segments/{segment_id}/data.rrd",
            get(recording_playback::segment),
        )
        .route(
            "/console/api/recordings/{recording_id}/playback-sessions/{playback_session}/segments/{segment_id}/live.rrd",
            get(recording_playback::live_segment),
        );
    let router = with_console_static_routes(router, config.asset_dir())?
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

fn with_console_static_routes<S>(router: Router<S>, asset_dir: &Path) -> anyhow::Result<Router<S>>
where
    S: Clone + Send + Sync + 'static,
{
    let index = Arc::<str>::from(
        fs::read_to_string(asset_dir.join("index.html")).with_context(|| {
            format!("reading Console entry document in {}", asset_dir.display())
        })?,
    );
    let immutable_assets = Router::<S>::new()
        .fallback_service(ServeDir::new(asset_dir.join("assets")))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ));
    let root_index = index;

    Ok(router
        // Hashed Vite assets are the only immutable Console surface. Their
        // dedicated route makes a missing bundle a real 404 instead of
        // serving index.html as JavaScript.
        .nest("/console/assets", immutable_assets)
        .route_service(
            "/console/favicon.svg",
            ServeFile::new(asset_dir.join("favicon.svg")),
        )
        .route(
            "/console",
            get(|| async { Redirect::permanent("/console/") }),
        )
        .route(
            "/console/",
            // The entry document names the current content-addressed bundles
            // and must be reloaded across a deployment.
            get(move || console_index(root_index.clone())),
        ))
}

async fn console_index(index: Arc<str>) -> impl axum::response::IntoResponse {
    (
        [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Html(index.to_string()),
    )
}

#[cfg(test)]
mod static_asset_tests {
    use std::fs;

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt as _;
    use uuid::Uuid;

    use super::*;

    fn fixture() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("veoveo-console-assets-{}", Uuid::now_v7()));
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(
            root.join("index.html"),
            r#"<script type="module" src="/console/assets/index-content.js"></script>"#,
        )
        .unwrap();
        fs::write(
            root.join("assets/index-content.js"),
            "document.body.textContent='ready';",
        )
        .unwrap();
        root
    }

    #[tokio::test]
    async fn shell_is_not_cached_and_hashed_assets_are_immutable() {
        let root = fixture();
        let router = with_console_static_routes(Router::new(), &root).unwrap();

        let root_shell = router
            .clone()
            .oneshot(Request::get("/console/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(root_shell.status(), StatusCode::OK);
        assert_eq!(root_shell.headers()[CACHE_CONTROL], "no-store");

        let non_root = router
            .clone()
            .oneshot(
                Request::get("/console/apps/example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(non_root.status(), StatusCode::NOT_FOUND);

        let asset = router
            .oneshot(
                Request::get("/console/assets/index-content.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(asset.status(), StatusCode::OK);
        assert_eq!(
            asset.headers()[CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn missing_hashed_asset_does_not_fall_back_to_html() {
        let root = fixture();
        let response = with_console_static_routes(Router::new(), &root)
            .unwrap()
            .oneshot(
                Request::get("/console/assets/index-missing.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        fs::remove_dir_all(root).unwrap();
    }
}
