//! Stateless HTTP mounting and signature admission. Domain handlers recheck current authority.
use crate::{embed::Embeddings, mcp::KnowledgeMcp};
use axum::{
    Router,
    extract::{Extension, Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use rmcp::transport::streamable_http_server::StreamableHttpService;
use std::sync::Arc;
use veoveo_mcp_contract::GatewayInternalTokenVerifier;

pub fn router<E: Embeddings + 'static>(
    server: KnowledgeMcp<E>,
    verifier: GatewayInternalTokenVerifier,
    allowed_hosts: Vec<String>,
    cancellation: tokio_util::sync::CancellationToken,
    readiness: crate::indexing::IndexingReadiness,
) -> Router {
    std::sync::LazyLock::force(&crate::mcp::SETUP);
    let store = server.store.clone();
    server.observe_catalog(cancellation.child_token());
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(cancellation),
    );
    let verifier_for_docs = verifier.clone();
    let mcp = Router::new()
        .route_service("/", service.clone())
        .route_service("/{*path}", service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn_with_state(verifier, authenticate));
    let admin = Router::new()
        .route("/docs/llms.txt", get(docs_index))
        .route("/docs/{document}", get(doc_body))
        .with_state(store.clone())
        .layer(middleware::from_fn_with_state(
            verifier_for_docs,
            authenticate,
        ));
    Router::new()
        .nest("/admin", admin)
        .nest("/mcp", mcp)
        .route("/livez", get(|| async { StatusCode::OK }))
        .route(
            "/healthz",
            get(move || {
                let store = store.clone();
                let readiness = readiness.clone();
                async move {
                    if readiness.is_ready()
                        && tokio::time::timeout(
                            std::time::Duration::from_secs(2),
                            store.active_gateway_control_revision(),
                        )
                        .await
                        .is_ok_and(|result| result.is_ok_and(|revision| revision.is_some()))
                    {
                        StatusCode::OK
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                }
            }),
        )
        .layer(middleware::from_fn_with_state(
            Arc::new(allowed_hosts),
            validate_host,
        ))
}
async fn authenticate(
    State(verifier): State<GatewayInternalTokenVerifier>,
    mut request: Request,
    next: Next,
) -> Response {
    let identity = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.split_once(' '))
        .filter(|(scheme, token)| {
            scheme.eq_ignore_ascii_case("bearer")
                && !token.is_empty()
                && !token.chars().any(char::is_whitespace)
        })
        .and_then(|(_, token)| verifier.verify(token).ok());
    let Some(identity) = identity else {
        return (
            StatusCode::UNAUTHORIZED,
            "valid gateway authorization required",
        )
            .into_response();
    };
    request.extensions_mut().insert(identity);
    next.run(request).await
}
async fn validate_host(
    State(allowed): State<Arc<Vec<String>>>,
    request: Request,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(veoveo_mcp_contract::parse_request_host_authority)
        .or_else(|| {
            request.uri().authority().and_then(|authority| {
                veoveo_mcp_contract::parse_request_host_authority(authority.as_str())
            })
        });
    if host
        .as_ref()
        .is_some_and(|host| veoveo_mcp_contract::host_authority_is_allowed(host, &allowed))
    {
        next.run(request).await
    } else {
        StatusCode::MISDIRECTED_REQUEST.into_response()
    }
}

async fn docs_index(
    State(store): State<veoveo_platform_store::PlatformStore>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
) -> Response {
    if !docs_allowed(
        &store,
        &identity,
        crate::contract::KnowledgeResource::Docs { after: None },
    )
    .await
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    use veoveo_mcp_contract::server_contract::McpServerContract;
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        crate::mcp::KnowledgeContract::documents().llms_txt(),
    )
        .into_response()
}
async fn doc_body(
    State(store): State<veoveo_platform_store::PlatformStore>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path(document): Path<String>,
) -> Response {
    let Ok(id) = document.parse() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !docs_allowed(
        &store,
        &identity,
        crate::contract::KnowledgeResource::Document(id),
    )
    .await
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    use veoveo_mcp_contract::server_contract::McpServerContract;
    match crate::mcp::KnowledgeContract::documents().doc(&document) {
        Some(doc) => (
            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
            doc.body,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn docs_allowed(
    store: &veoveo_platform_store::PlatformStore,
    identity: &veoveo_mcp_contract::GatewayInternalIdentity,
    address: crate::contract::KnowledgeResource,
) -> bool {
    use veoveo_types::ResourceAddress;
    let Ok(uri) = address.to_uri() else {
        return false;
    };
    crate::authority::authorize(
        store,
        identity,
        crate::contract::KnowledgeScope::Read,
        veoveo_mcp_contract::GatewayAction::ResourcesRead,
        &veoveo_mcp_contract::PolicyTarget::Resource {
            server: "knowledge".parse().expect("server slug"),
            uri,
        },
    )
    .await
    .is_ok()
}
