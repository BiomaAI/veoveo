use axum::{
    Extension, Router,
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use veoveo_mcp_contract::GatewayInternalIdentity;

use crate::mcp::SERVER_DOCS;

pub(super) fn router() -> Router {
    Router::new()
        .route("/docs/llms.txt", get(docs_index))
        .route("/docs/{doc_id}", get(doc_body))
}

async fn docs_index(Extension(identity): Extension<GatewayInternalIdentity>) -> Response {
    if !may_read_docs(&identity) {
        return StatusCode::FORBIDDEN.into_response();
    }
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        SERVER_DOCS.llms_txt(),
    )
        .into_response()
}

async fn doc_body(
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(doc_id): Path<String>,
) -> Response {
    if !may_read_docs(&identity) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match SERVER_DOCS.doc(&doc_id) {
        Some(doc) => (
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            doc.body,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "unknown Simulation View document").into_response(),
    }
}

fn may_read_docs(identity: &GatewayInternalIdentity) -> bool {
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == "simulation-view:read")
}
