use anyhow::Context;
use axum::{
    Router,
    http::{HeaderValue, header::CACHE_CONTROL},
    response::{Html, Redirect},
    routing::get,
};
use std::{path::Path, sync::Arc};
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

pub(crate) fn routes<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    directory: &Path,
) -> anyhow::Result<Router<S>> {
    let index = Arc::new(
        std::fs::read_to_string(directory.join("index.html"))
            .with_context(|| format!("reading Workspace entry in {}", directory.display()))?,
    );
    let assets = Router::<S>::new()
        .fallback_service(ServeDir::new(directory.join("assets")))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ));
    Ok(router
        .nest("/workspace/assets", assets)
        .route(
            "/workspace",
            get(|| async { Redirect::permanent("/workspace/") }),
        )
        .route(
            "/workspace/",
            get(move || {
                let index = index.clone();
                async move {
                    (
                        [
                            (CACHE_CONTROL, "no-store"),
                            (
                                axum::http::header::HeaderName::from_static("permissions-policy"),
                                "camera=(), microphone=(self), geolocation=(), payment=()",
                            ),
                        ],
                        Html(index.as_str().to_owned()),
                    )
                }
            }),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn deployed_entry_is_fresh_and_missing_assets_do_not_return_html() {
        let directory =
            std::env::temp_dir().join(format!("workspace-assets-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(directory.join("assets")).unwrap();
        std::fs::write(directory.join("index.html"), "<title>Workspace</title>").unwrap();
        std::fs::write(directory.join("assets/entry-hash.js"), "export {};").unwrap();
        let app = routes(Router::new(), &directory).unwrap();
        for (path, status, cache) in [
            ("/workspace/", StatusCode::OK, "no-store"),
            (
                "/workspace/assets/entry-hash.js",
                StatusCode::OK,
                "public, max-age=31536000, immutable",
            ),
            (
                "/workspace/assets/missing.js",
                StatusCode::NOT_FOUND,
                "public, max-age=31536000, immutable",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            assert_eq!(response.headers()[CACHE_CONTROL], cache);
            if path == "/workspace/" {
                assert_eq!(
                    response.headers()["permissions-policy"],
                    "camera=(), microphone=(self), geolocation=(), payment=()"
                );
            }
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
