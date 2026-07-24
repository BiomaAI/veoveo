//! Administrative REST projection of the well-known surface.
//!
//! Serves the crate documents embedded at build time for REST consumers at
//! `{mount}/admin/docs/llms.txt` and `{mount}/admin/docs/{doc_id}` (contract
//! C20, C21). The server nests this router behind the same gateway
//! authentication and administrative scope authorization as every other
//! administrative surface.

mod handlers;

use axum::{Router, routing::get};

pub(crate) fn router() -> Router {
    Router::new()
        .route("/docs/llms.txt", get(handlers::docs_index))
        .route("/docs/{doc_id}", get(handlers::doc_body))
}
