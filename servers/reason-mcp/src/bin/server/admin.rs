//! Administrative REST projection of the well-known surface.
//!
//! Serves the crate documents embedded at build time for REST consumers at
//! `{mount}/admin/docs/llms.txt` and `{mount}/admin/docs/{doc_id}` (contract
//! C20, C21). The server nests this router behind the same gateway
//! authentication as the MCP endpoint.

use axum::{
    Router,
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use super::SERVER_DOCS;

pub(super) fn router() -> Router {
    Router::new()
        .route("/docs/llms.txt", get(docs_index))
        .route("/docs/{doc_id}", get(doc_body))
}

/// `GET {mount}/admin/docs/llms.txt` (contract C20).
async fn docs_index() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        SERVER_DOCS.llms_txt(),
    )
        .into_response()
}

/// `GET {mount}/admin/docs/{doc_id}` (contract C20).
async fn doc_body(Path(doc_id): Path<String>) -> Response {
    match SERVER_DOCS.doc(&doc_id) {
        Some(doc) => (
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            doc.body,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "unknown Reason document").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn respond(uri: &str) -> (axum::http::StatusCode, String, String) {
        let response = super::router()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .map(|value| value.to_str().unwrap().to_owned())
            .unwrap_or_default();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            content_type,
            String::from_utf8(bytes.to_vec()).unwrap(),
        )
    }

    #[tokio::test]
    async fn llms_txt_indexes_the_embedded_documents() {
        let (status, content_type, body) = respond("/docs/llms.txt").await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(content_type, "text/plain; charset=utf-8");
        assert!(body.starts_with("# reason\n"));
        assert!(body.contains("(agents)"));
        assert!(body.contains("(design)"));
    }

    #[tokio::test]
    async fn document_bodies_serve_the_embedded_markdown() {
        let (status, content_type, body) = respond("/docs/agents").await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(content_type, "text/markdown; charset=utf-8");
        assert!(body.contains("## Contract Compliance"));

        let (status, _, _) = respond("/docs/unknown").await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }
}
