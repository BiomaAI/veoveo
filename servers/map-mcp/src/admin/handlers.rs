use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::mcp::SERVER_DOCS;

/// `GET {mount}/admin/docs/llms.txt` (contract C20).
pub(super) async fn docs_index() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        SERVER_DOCS.llms_txt(),
    )
        .into_response()
}

/// `GET {mount}/admin/docs/{doc_id}` (contract C20).
pub(super) async fn doc_body(Path(doc_id): Path<String>) -> Response {
    match SERVER_DOCS.doc(&doc_id) {
        Some(doc) => (
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            doc.body,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "unknown Map document").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn respond(uri: &str) -> (axum::http::StatusCode, String, String) {
        let response = super::super::router()
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
        assert!(body.starts_with("# map\n"));
        assert!(body.contains("(docs/agents)"));
        assert!(body.contains("(docs/design)"));
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
