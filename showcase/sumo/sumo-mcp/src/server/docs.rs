//! Embedded owner documents share the SUMO forwarded-identity boundary.
use super::{
    auth::{InternalMcpAuthState, authenticate_internal_mcp},
    ownership::internal_identity,
};
use axum::{
    Extension, Router,
    extract::Path,
    http::{StatusCode, header},
    middleware,
    response::IntoResponse,
    routing::get,
};
use rmcp::{
    ErrorData, RoleServer,
    model::{
        ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
        ResourceContents, ResourceTemplate,
    },
    service::RequestContext,
};
use std::sync::LazyLock;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, GatewayInternalTokenVerifier,
    docs::{ServerDocs, knowledge_extension},
};
use veoveo_types::ResourceScheme;

pub(super) static DOCUMENTS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("sumo"));
/// Keep document admission ahead of every simulator, publication and Task effect.
pub(super) async fn with_admitted_documents<T, F, Fut>(
    documents: &ServerDocs,
    start: F,
) -> anyhow::Result<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let profile = documents.admitted_profile()?;
    let configuration = super::service::server_configuration(documents);
    anyhow::ensure!(
        profile.server().as_str() == configuration.server_info.name,
        "document profile/server identity disagrees with Discover"
    );
    let declared = configuration
        .capabilities
        .extensions
        .as_ref()
        .is_some_and(|extensions| extensions.contains_key(knowledge_extension::EXTENSION_ID));
    profile.check_knowledge_applicability(declared)?;
    start().await
}

fn scheme() -> ResourceScheme {
    ResourceScheme::parse("sumo").expect("fixed SUMO scheme")
}

pub(super) fn resources() -> Vec<Resource> {
    vec![
        Resource::new("sumo://docs", "server documents").with_mime_type("application/json"),
        Resource::new("sumo://contract", "contract declaration").with_mime_type("application/json"),
    ]
}
pub(super) fn templates() -> Vec<ResourceTemplate> {
    let mut template = ResourceTemplate::new(
        knowledge_extension::docs::member_template(&scheme()).as_str(),
        "server document",
    );
    DOCUMENTS
        .knowledge_template(&scheme(), &mut template)
        .expect("fixed owner document template");
    vec![template]
}
pub(super) fn read(
    request: &ReadResourceRequestParams,
    context: &RequestContext<RoleServer>,
) -> Result<Option<ReadResourceResponse>, ErrorData> {
    internal_identity(context)?;
    if let Some(result) = DOCUMENTS.read_knowledge(&scheme(), request, context)? {
        return Ok(Some(result));
    }
    if request.uri == "sumo://contract" {
        let text = serde_json::to_string(DOCUMENTS.contract_declaration()).map_err(|_| {
            ErrorData::internal_error("contract declaration serialization failed", None)
        })?;
        let result = ReadResourceResult::new(vec![
            ResourceContents::text(text, &request.uri).with_mime_type("application/json"),
        ]);
        return Ok(Some(veoveo_mcp_contract::private_resource_response(
            result,
            request.request_state.is_none() && request.input_responses.is_none(),
        )));
    }
    Ok(None)
}

/// The same verifier and authentication middleware guard both administrative projections.
pub(super) fn admin_router(verifier: GatewayInternalTokenVerifier) -> Router {
    Router::new()
        .route(
            "/docs/llms.txt",
            get(
                |Extension(_identity): Extension<GatewayInternalIdentity>| async {
                    (
                        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                        DOCUMENTS.llms_txt(),
                    )
                        .into_response()
                },
            ),
        )
        .route(
            "/docs/{doc_id}",
            get(
                |Extension(_identity): Extension<GatewayInternalIdentity>,
                 Path(id): Path<String>| async move {
                    match DOCUMENTS.doc(&id) {
                        Some(doc) => (
                            [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
                            doc.body,
                        )
                            .into_response(),
                        None => (StatusCode::NOT_FOUND, "unknown document").into_response(),
                    }
                },
            ),
        )
        .layer(middleware::from_fn_with_state(
            InternalMcpAuthState { verifier },
            authenticate_internal_mcp,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use chrono::{TimeDelta, Utc};
    use tower::ServiceExt;
    use veoveo_mcp_contract::{
        GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenIssuer, GatewayProfileId, TokenIssuer,
        hosting::testing,
    };
    use veoveo_types::ServerSlug;

    async fn response(router: Router, request: Request<Body>) -> axum::response::Response {
        tokio::time::timeout(std::time::Duration::from_secs(5), router.oneshot(request))
            .await
            .expect("document response deadline")
            .unwrap()
    }

    #[tokio::test]
    async fn startup_rejects_contradictory_knowledge_before_any_owner_effect() {
        let mut profile: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/contract-compliance.json"
        )))
        .unwrap();
        let item = profile["compliance"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|item| item["id"] == "C32")
            .unwrap();
        item["status"] = serde_json::json!("not_applicable");
        item["note"] = serde_json::json!("Knowledge extension is not declared.");
        let bytes = profile.to_string();
        let admitted: veoveo_mcp_contract::docs::ComplianceProfile =
            serde_json::from_str(&bytes).unwrap();
        let manual: &'static str = Box::leak(
            format!(
                "# SUMO\n\n## Contract Compliance\n\n{}\n",
                veoveo_mcp_contract::docs::render_compliance(&admitted)
            )
            .into_boxed_str(),
        );
        let contradictory = ServerDocs::new("sumo")
            .with_doc("agents", "Agent manual", manual)
            .with_doc("design", "Design", DOCUMENTS.doc("design").unwrap().body)
            .with_profile_json(&bytes)
            .unwrap();
        let effects = std::cell::Cell::new((0, 0, 0, 0));
        let rejected = with_admitted_documents(&contradictory, || async {
            effects.set((1, 1, 1, 1)); // simulator, publisher, Tasks, recovery
            Ok(())
        })
        .await;
        assert!(rejected.is_err());
        assert_eq!(effects.get(), (0, 0, 0, 0));
        with_admitted_documents(&DOCUMENTS, || async {
            effects.set((1, 1, 1, 1));
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(effects.get(), (1, 1, 1, 1));
    }
    #[tokio::test]
    async fn signed_admin_docs_serve_exact_embedded_bytes_and_reject_foreign_identity() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let issuer = TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap();
        let router = admin_router(GatewayInternalTokenVerifier::new(
            issuer.clone(),
            ServerSlug::parse("sumo").unwrap(),
            testing::trust_bundle("docs-key"),
        ));
        for (audience, expected) in [
            ("sumo", StatusCode::OK),
            ("foreign", StatusCode::UNAUTHORIZED),
        ] {
            let bearer =
                GatewayInternalTokenIssuer::new(issuer.clone(), testing::signing_key("docs-key"))
                    .issue(
                        GatewayProfileId::parse("operations").unwrap(),
                        ServerSlug::parse(audience).unwrap(),
                        testing::principal(),
                        testing::authority(),
                        None,
                        Utc::now() + TimeDelta::minutes(5),
                    )
                    .unwrap()
                    .bearer_token;
            for (path, body) in [
                ("/docs/llms.txt", DOCUMENTS.llms_txt()),
                (
                    "/docs/agents",
                    DOCUMENTS.doc("agents").unwrap().body.to_owned(),
                ),
                (
                    "/docs/design",
                    DOCUMENTS.doc("design").unwrap().body.to_owned(),
                ),
            ] {
                let request = Request::builder()
                    .uri(path)
                    .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
                    .body(Body::empty())
                    .unwrap();
                let response = response(router.clone(), request).await;
                assert_eq!(response.status(), expected);
                if expected == StatusCode::OK {
                    assert_eq!(
                        to_bytes(response.into_body(), 65536)
                            .await
                            .unwrap()
                            .as_ref(),
                        body.as_bytes()
                    );
                }
            }
        }
        assert_eq!(
            response(
                router,
                Request::builder()
                    .uri("/docs/agents")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    #[derive(Clone)]
    struct DocumentationFixture;
    impl rmcp::ServerHandler for DocumentationFixture {
        fn supported_protocol_versions(
            &self,
        ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
            veoveo_mcp_contract::final_protocol_versions()
        }
        fn get_info(&self) -> rmcp::model::ServerConfig {
            let mut info = rmcp::model::ServerConfig::default();
            info.server_info = rmcp::model::Implementation::new("sumo", "fixture");
            info.capabilities = rmcp::model::ServerCapabilities::builder()
                .enable_resources()
                .build();
            DOCUMENTS.declare_knowledge(&mut info.capabilities);
            info
        }
        async fn read_resource(
            &self,
            request: ReadResourceRequestParams,
            context: RequestContext<RoleServer>,
        ) -> Result<ReadResourceResponse, ErrorData> {
            read(&request, &context)?
                .ok_or_else(|| ErrorData::resource_not_found("unknown document resource", None))
        }
    }

    #[tokio::test]
    async fn signed_mcp_reads_use_the_production_document_adapter() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let issuer = TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap();
        let verifier = GatewayInternalTokenVerifier::new(
            issuer.clone(),
            ServerSlug::parse("sumo").unwrap(),
            testing::trust_bundle("docs-key"),
        );
        let service = rmcp::transport::streamable_http_server::StreamableHttpService::new(
            || Ok(DocumentationFixture),
            veoveo_mcp_contract::stateless_session_manager(),
            veoveo_mcp_contract::canonical_streamable_http_server_config(),
        );
        let router =
            Router::new()
                .route_service("/mcp", service)
                .layer(middleware::from_fn_with_state(
                    InternalMcpAuthState { verifier },
                    authenticate_internal_mcp,
                ));
        for (audience, host, expected) in [
            ("sumo", Some("localhost"), StatusCode::OK),
            ("foreign", Some("localhost"), StatusCode::UNAUTHORIZED),
            ("sumo", None, StatusCode::BAD_REQUEST),
            ("sumo", Some("invalid host"), StatusCode::BAD_REQUEST),
        ] {
            let token =
                GatewayInternalTokenIssuer::new(issuer.clone(), testing::signing_key("docs-key"))
                    .issue(
                        GatewayProfileId::parse("operations").unwrap(),
                        ServerSlug::parse(audience).unwrap(),
                        testing::principal(),
                        testing::authority(),
                        None,
                        Utc::now() + TimeDelta::minutes(5),
                    )
                    .unwrap()
                    .bearer_token;
            for uri in [
                "sumo://docs",
                "sumo://docs/agents",
                "sumo://docs/design",
                "sumo://contract",
            ] {
                let body = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":uri,"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"sumo-docs-test","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}}}});
                let request = Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("accept", "application/json, text/event-stream")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("mcp-protocol-version", "2026-07-28")
                    .header("mcp-method", "resources/read")
                    .header("mcp-name", uri)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"));
                // The pinned transport validates Host before resource dispatch, as
                // does TestGateway. Keep its canonical localhost allowlist intact.
                let request = if let Some(host) = host {
                    request.header(header::HOST, host)
                } else {
                    request
                };
                let request = request.body(Body::from(body.to_string())).unwrap();
                let response = response(router.clone(), request).await;
                let status = response.status();
                let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
                // Response-only diagnostics never include the signed request/token.
                let diagnostic = String::from_utf8_lossy(&bytes);
                assert_eq!(
                    status, expected,
                    "document response for {uri}, audience {audience}, Host {host:?}: {diagnostic}"
                );
                if expected == StatusCode::BAD_REQUEST {
                    assert!(diagnostic.contains("Host header"), "{diagnostic}");
                }
                if expected != StatusCode::OK {
                    continue;
                }
                let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert!(response.get("error").is_none(), "{response}");
                let text = response["result"]["contents"][0]["text"].as_str().unwrap();
                match uri {
                    "sumo://contract" => assert_eq!(
                        serde_json::from_str::<serde_json::Value>(text).unwrap(),
                        serde_json::to_value(DOCUMENTS.contract_declaration()).unwrap()
                    ),
                    "sumo://docs/agents" => assert_eq!(text, DOCUMENTS.doc("agents").unwrap().body),
                    "sumo://docs/design" => assert_eq!(text, DOCUMENTS.doc("design").unwrap().body),
                    _ => assert_eq!(
                        serde_json::from_str::<serde_json::Value>(text).unwrap()["items"]
                            .as_array()
                            .unwrap()
                            .len(),
                        2
                    ),
                }
            }
        }
    }

    #[test]
    fn owner_well_known_contract_and_knowledge_template_are_complete() {
        let profile = DOCUMENTS.contract_declaration();
        assert_eq!(profile.server().as_str(), "sumo");
        assert_eq!(profile.compliance().len(), 32);
        assert!(
            profile
                .profile()
                .check_knowledge_applicability(true)
                .is_ok()
        );
        for id in [
            veoveo_mcp_contract::docs::RequirementId::C18,
            veoveo_mcp_contract::docs::RequirementId::C19,
            veoveo_mcp_contract::docs::RequirementId::C20,
            veoveo_mcp_contract::docs::RequirementId::C21,
        ] {
            assert_eq!(
                profile.profile().item(id).status,
                veoveo_mcp_contract::docs::ComplianceStatus::Met
            );
        }
        assert_eq!(resources().len(), 2);
        assert!(
            templates()[0]
                .meta
                .as_ref()
                .unwrap()
                .contains_key(knowledge_extension::EXTENSION_ID)
        );
    }
}
