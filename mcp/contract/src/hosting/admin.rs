//! Administrative HTTP projection of the well-known documents (contract C20, C21).
//!
//! `{mount}/admin/docs/llms.txt` indexes the embedded documents and
//! `{mount}/admin/docs/{doc_id}` serves one body. The hosting builder nests this
//! router behind the same gateway authentication as the MCP endpoint, and the
//! domain's document authorization decides each request.

use std::sync::Arc;

use axum::{
    Extension, Router,
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures::future::BoxFuture;
use rmcp::ErrorData;
use veoveo_types::{ResourceScheme, ResourceUri, ResourceUriBuilder, UriSegment};

use crate::{GatewayInternalIdentity, docs::ServerDocs};

/// The domain's document authorization for one caller and document address.
pub(super) type DocumentAuthorizer = Arc<
    dyn Fn(GatewayInternalIdentity, ResourceUri) -> BoxFuture<'static, Result<(), ErrorData>>
        + Send
        + Sync,
>;

pub(super) fn docs_router(
    docs: &'static ServerDocs,
    scheme: ResourceScheme,
    authorize: DocumentAuthorizer,
) -> Router {
    let index = crate::docs::knowledge_extension::docs::index_uri(&scheme);
    let index_authorize = authorize.clone();
    Router::new()
        .route(
            "/docs/llms.txt",
            get(
                move |Extension(identity): Extension<GatewayInternalIdentity>| {
                    let (authorize, index) = (index_authorize.clone(), index.clone());
                    async move {
                        if authorize(identity, index).await.is_err() {
                            return StatusCode::FORBIDDEN.into_response();
                        }
                        (
                            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                            docs.llms_txt(),
                        )
                            .into_response()
                    }
                },
            ),
        )
        .route(
            "/docs/{doc_id}",
            get(
                move |Extension(identity): Extension<GatewayInternalIdentity>,
                      Path(doc_id): Path<String>| {
                    let (authorize, scheme) = (authorize.clone(), scheme.clone());
                    async move {
                        let Some(address) = member_address(&scheme, &doc_id) else {
                            return (StatusCode::NOT_FOUND, "unknown document").into_response();
                        };
                        if authorize(identity, address).await.is_err() {
                            return StatusCode::FORBIDDEN.into_response();
                        }
                        doc_body(docs, &doc_id)
                    }
                },
            ),
        )
}

fn member_address(scheme: &ResourceScheme, doc_id: &str) -> Option<ResourceUri> {
    let index = crate::docs::knowledge_extension::docs::index_uri(scheme);
    ResourceUriBuilder::new(index.as_str())
        .ok()?
        .segment(UriSegment::new(doc_id).ok()?)
        .build()
        .ok()
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
