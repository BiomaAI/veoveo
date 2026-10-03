//! Administrative HTTP projection of the well-known documents (contract C20, C21).
//!
//! `{mount}/admin/docs/llms.txt` indexes the embedded documents and
//! `{mount}/admin/docs/{doc_id}` serves one body. The hosting builder nests this
//! router behind the same gateway authentication as the MCP endpoint.

use axum::{
    Router,
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use crate::docs::ServerDocs;

pub(super) fn docs_router(docs: &'static ServerDocs) -> Router {
    Router::new()
        .route(
            "/docs/llms.txt",
            get(move || async move {
                (
                    [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                    docs.llms_txt(),
                )
                    .into_response()
            }),
        )
        .route(
            "/docs/{doc_id}",
            get(move |Path(doc_id): Path<String>| async move { doc_body(docs, &doc_id) }),
        )
}

fn doc_body(docs: &ServerDocs, doc_id: &str) -> Response {
    match docs.doc(doc_id) {
        Some(doc) => (
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            doc.body,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "unknown document").into_response(),
    }
}
