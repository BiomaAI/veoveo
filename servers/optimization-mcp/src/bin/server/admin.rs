use axum::{
    Router,
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use super::service::SERVER_DOCS;

pub(super) fn router() -> Router {
    Router::new()
        .route("/docs/llms.txt", get(docs_index))
        .route("/docs/{doc_id}", get(doc_body))
}

async fn docs_index() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        SERVER_DOCS.llms_txt(),
    )
        .into_response()
}

async fn doc_body(Path(doc_id): Path<String>) -> Response {
    match SERVER_DOCS.doc(&doc_id) {
        Some(doc) => (
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            doc.body,
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "unknown Optimization server document",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn llms_index_and_manual_are_embedded() {
        let response = super::router()
            .oneshot(
                Request::builder()
                    .uri("/docs/llms.txt")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let response = super::router()
            .oneshot(
                Request::builder()
                    .uri("/docs/agents")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }
}
