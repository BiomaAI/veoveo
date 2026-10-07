use std::{collections::BTreeSet, sync::LazyLock};

use axum::{Router, response::IntoResponse};
use rmcp::{
    RoleServer, ServerHandler,
    model::{
        Implementation, JsonObject, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
        ResourceContents, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
    transport::streamable_http_server::StreamableHttpService,
};
use veoveo_mcp_conformance::{
    ConformanceCredentials, HostedServerConformanceProfile, HostedServerProfileSchema,
    HttpBoundaryProfile, SurfaceExpectation, SurfaceProfile, run_hosted_server_conformance,
};
use veoveo_mcp_contract::docs::{ContractDeclaration, DOC_ID_AGENTS, DOC_ID_DESIGN, ServerDocs};

const FIXTURE_MANUAL: &str = include_str!("../../contract/testdata/compliance-domain.md");

static FIXTURE_DOCS: LazyLock<ServerDocs> = LazyLock::new(|| {
    ServerDocs::new("domain")
        .with_doc(DOC_ID_AGENTS, "Agent work manual", FIXTURE_MANUAL)
        .with_doc(
            DOC_ID_DESIGN,
            "Domain design",
            "# Domain design\n\nFixture.",
        )
        .with_profile_json(include_str!(
            "../../contract/testdata/compliance-domain.json"
        ))
        .unwrap()
});
static FIXTURE_DECLARATION: LazyLock<ContractDeclaration> =
    LazyLock::new(|| ContractDeclaration::from_docs(&FIXTURE_DOCS));

#[derive(Clone)]
struct DomainFixture(std::sync::Arc<std::sync::atomic::AtomicU8>);

impl DomainFixture {
    fn degradation(
        &self,
        last: bool,
        surface: veoveo_gateway_contract::GatewayDiscoverySurface,
    ) -> Option<rmcp::model::MetaObject> {
        use veoveo_mcp_contract::GatewayDiscoveryMetadata;
        let mode = self.0.load(std::sync::atomic::Ordering::SeqCst);
        let selected = match mode {
            4 => !last,
            5 => last,
            6 => true,
            _ => false,
        };
        if selected {
            return veoveo_gateway_contract::GatewayDiscoveryDegradation::new([
                veoveo_gateway_contract::GatewayDiscoveryFailure {
                    server: veoveo_types::ServerSlug::parse("unavailable").unwrap(),
                    surface,
                    code: if last {
                        veoveo_gateway_contract::GatewayDiscoveryFailureCode::DiscoveryPending
                    } else {
                        veoveo_gateway_contract::GatewayDiscoveryFailureCode::UpstreamUnavailable
                    },
                },
            ])
            .into_meta();
        }
        if mode == 7 {
            let mut meta = rmcp::model::MetaObject::new();
            meta.insert(veoveo_gateway_contract::GATEWAY_DISCOVERY_DEGRADATION_META_KEY.into(), serde_json::json!({"failures":[{"server":"unavailable","surface":"tools","code":"invented"}]}));
            return Some(meta);
        }
        None
    }
}

impl ServerHandler for DomainFixture {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        if self.0.load(std::sync::atomic::Ordering::SeqCst) == 3 {
            info.capabilities.extensions.get_or_insert_default().insert(
                veoveo_mcp_contract::docs::knowledge_extension::EXTENSION_ID.into(),
                JsonObject::new(),
            );
        }
        info.server_info = Implementation::new("domain", "1.0.0");
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let schema: JsonObject = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "value": {"type": "string"}
            },
            "additionalProperties": false
        }))
        .unwrap();
        let last = request.and_then(|request| request.cursor).is_some();
        let mut tool = Tool::new(
            if last { "inspect" } else { "first_page" },
            "Inspect one value.",
            schema,
        );
        tool.output_schema = Some(std::sync::Arc::new(serde_json::from_value(
            serde_json::json!({"type":"object","properties":{"value":{"type":"string"}},"additionalProperties":false})
        ).unwrap()));
        Ok(ListToolsResult {
            tools: vec![tool],
            next_cursor: (!last).then(|| "tools-last".to_owned()),
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: self.degradation(
                last,
                veoveo_gateway_contract::GatewayDiscoverySurface::Tools,
            ),
        })
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        let last = request.and_then(|request| request.cursor).is_some();
        Ok(ListResourcesResult {
            resources: if last {
                vec![
                    Resource::new("domain://docs/agents", "agent work manual"),
                    Resource::new("domain://docs/design", "domain design"),
                    Resource::new("domain://contract", "contract declaration"),
                ]
            } else {
                vec![Resource::new("domain://docs", "contract documentation")]
            },
            next_cursor: (!last).then(|| "resources-last".to_owned()),
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: self.degradation(
                last,
                veoveo_gateway_contract::GatewayDiscoverySurface::Resources,
            ),
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, rmcp::ErrorData> {
        let uri = request.uri.as_str();
        if uri == "domain://docs" {
            let entries = FIXTURE_DOCS
                .document_page(
                    &veoveo_types::ResourceScheme::parse("domain").unwrap(),
                    None,
                )
                .unwrap();
            let text = serde_json::to_string(&entries).expect("doc index serializes");
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri)]).into());
        }
        if let Some(id) = uri.strip_prefix("domain://docs/")
            && let Some(doc) = FIXTURE_DOCS.doc(id)
        {
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(doc.body, uri)]).into());
        }
        if uri == "domain://contract" {
            let mut value = serde_json::to_value(&*FIXTURE_DECLARATION).unwrap();
            match self.0.load(std::sync::atomic::Ordering::SeqCst) {
                1 => {
                    value["compliance"].as_array_mut().unwrap().pop();
                }
                2 => {
                    value["compliance"][31]["status"] = serde_json::json!("pending");
                }
                _ => {}
            }
            let text = serde_json::to_string(&value).expect("declaration serializes");
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(text, uri)]).into());
        }
        Err(rmcp::ErrorData::invalid_params("unknown resource", None))
    }
}

#[tokio::test]
async fn certifies_a_domain_without_linking_its_implementation() -> anyhow::Result<()> {
    let mode = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    let factory_mode = mode.clone();
    let service: StreamableHttpService<
        DomainFixture,
        rmcp::transport::streamable_http_server::session::never::NeverSessionManager,
    > = StreamableHttpService::new(
        move || Ok(DomainFixture(factory_mode.clone())),
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let admin = Router::new()
        .route(
            "/domain/admin/docs/llms.txt",
            axum::routing::get(|headers: axum::http::HeaderMap| async move {
                if !authorized(&headers) {
                    return (axum::http::StatusCode::UNAUTHORIZED, String::new());
                }
                (axum::http::StatusCode::OK, FIXTURE_DOCS.llms_txt())
            }),
        )
        .route(
            "/domain/admin/docs/{id}",
            axum::routing::get(
                |axum::extract::Path(id): axum::extract::Path<String>,
                 headers: axum::http::HeaderMap| async move {
                    if !authorized(&headers) {
                        return (axum::http::StatusCode::UNAUTHORIZED, String::new());
                    }
                    match FIXTURE_DOCS.doc(&id) {
                        Some(doc) => (axum::http::StatusCode::OK, doc.body.to_owned()),
                        None => (axum::http::StatusCode::NOT_FOUND, String::new()),
                    }
                },
            ),
        );
    let mcp = Router::new()
        .nest_service("/domain/mcp", service)
        .route_layer(axum::middleware::from_fn(
            |request: axum::extract::Request, next: axum::middleware::Next| async move {
                if authorized(request.headers()) {
                    next.run(request).await
                } else {
                    axum::http::StatusCode::UNAUTHORIZED.into_response()
                }
            },
        ));
    let server =
        tokio::spawn(
            async move { axum::serve(listener, Router::new().merge(mcp).merge(admin)).await },
        );

    let profile = HostedServerConformanceProfile {
        schema_version: HostedServerProfileSchema::V2,
        profile_id: "anonymous-extension".to_owned(),
        contract_revision: "veoveo.ai/hosted-mcp/v4".to_owned(),
        endpoint: format!("http://{address}/domain/mcp"),
        server_slug: "domain".to_owned(),
        owned_resource_schemes: BTreeSet::from(["domain".to_owned()]),
        http: HttpBoundaryProfile {
            require_authentication_rejection: true,
            rejected_host: None,
            health_url: None,
            readiness_url: None,
            docs_llms_url: format!("http://{address}/domain/admin/docs/llms.txt"),
        },
        surfaces: SurfaceProfile {
            tools: SurfaceExpectation::Required,
            resources: SurfaceExpectation::Required,
            resource_templates: SurfaceExpectation::Optional,
            prompts: SurfaceExpectation::Optional,
            completions: SurfaceExpectation::Optional,
            tasks: SurfaceExpectation::Optional,
            subscriptions: SurfaceExpectation::Optional,
            required_tools: BTreeSet::from(["inspect".to_owned()]),
            required_resources: BTreeSet::from(["domain://docs".to_owned()]),
            required_resource_templates: BTreeSet::new(),
            required_prompts: BTreeSet::new(),
        },
    };
    let report =
        run_hosted_server_conformance(&profile, &ConformanceCredentials::bearer("test-token"))
            .await?;
    assert!(report.passed(), "{:#?}", report.checks);
    let source = veoveo_mcp_conformance::run_knowledge_source_conformance(
        &veoveo_mcp_conformance::KnowledgeSourceTarget::try_from(&profile)?,
        &ConformanceCredentials::bearer("test-token"),
        &Default::default(),
    )
    .await?;
    // This hosted fixture intentionally omits the extension. Source-only
    // qualification must fail instead of succeeding with every K check skipped.
    assert!(!source.passed());
    assert!(
        source
            .checks
            .iter()
            .any(|check| check.requirement_id == "K01"
                && check.status == veoveo_mcp_conformance::CheckStatus::Failed)
    );
    assert_eq!(
        report
            .implementation
            .as_ref()
            .map(|value| value.name.as_str()),
        Some("domain")
    );

    for (variant, failed_check) in [
        (1, "VV-MCP-CONTRACT-001"),
        (2, "VV-MCP-CONTRACT-003"),
        (3, "VV-MCP-CONTRACT-003"),
    ] {
        mode.store(variant, std::sync::atomic::Ordering::SeqCst);
        let rejected =
            run_hosted_server_conformance(&profile, &ConformanceCredentials::bearer("test-token"))
                .await?;
        assert!(!rejected.passed());
        assert!(
            rejected
                .checks
                .iter()
                .any(|check| check.requirement_id == failed_check
                    && check.status == veoveo_mcp_conformance::CheckStatus::Failed),
            "{:#?}",
            rejected.checks
        );
    }
    for variant in [4, 5, 6, 7] {
        mode.store(variant, std::sync::atomic::Ordering::SeqCst);
        let report =
            run_hosted_server_conformance(&profile, &ConformanceCredentials::bearer("test-token"))
                .await?;
        assert!(!report.passed());
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.requirement_id == "VV-MCP-NAMING-001"
                    && check.status == veoveo_mcp_conformance::CheckStatus::Incomplete),
            "degraded or malformed page mode {variant}: {:#?}",
            report.checks
        );
    }
    server.abort();
    Ok(())
}

fn authorized(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        == Some("Bearer test-token")
}
