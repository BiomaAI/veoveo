use std::{collections::BTreeSet, sync::LazyLock};

use axum::Router;
use rmcp::{
    RoleServer, ServerHandler,
    model::{
        Implementation, JsonObject, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
        ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        StreamableHttpService, session::local::LocalSessionManager,
    },
};
use veoveo_mcp_conformance::{
    ConformanceCredentials, HostedServerConformanceProfile, HostedServerProfileSchema,
    HttpBoundaryProfile, SurfaceExpectation, SurfaceProfile, run_hosted_server_conformance,
};
use veoveo_mcp_contract::docs::{
    CapabilityInventory, ContractDeclaration, DOC_ID_AGENTS, DOC_ID_DESIGN, ServerDocs,
};

const FIXTURE_MANUAL: &str = "# Domain\n\n## Purpose\n\nConformance fixture.\n\n\
## Invariants\n\nNone.\n\n## Build And Test\n\ncargo test\n\n\
## Contract Compliance\n\n- C18: met\n- C19: met\n- C20: met\n- C21: met\n";

static FIXTURE_DOCS: LazyLock<ServerDocs> = LazyLock::new(|| {
    ServerDocs::new("domain")
        .with_doc(DOC_ID_AGENTS, "Agent work manual", FIXTURE_MANUAL)
        .with_doc(
            DOC_ID_DESIGN,
            "Domain design",
            "# Domain design\n\nFixture.",
        )
});

#[derive(Clone)]
struct DomainFixture;

impl ServerHandler for DomainFixture {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        info.server_info = Implementation::new("domain", "1.0.0");
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
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
        Ok(ListToolsResult {
            tools: vec![Tool::new("inspect", "Inspect one value.", schema)],
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        Ok(ListResourcesResult {
            resources: vec![
                Resource::new("domain://docs", "contract documentation"),
                Resource::new("domain://contract", "contract declaration"),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        let uri = request.uri.as_str();
        if uri == "domain://docs" {
            let entries: Vec<_> = FIXTURE_DOCS.iter().collect();
            let text = serde_json::to_string(&entries).expect("doc index serializes");
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text, uri,
            )]));
        }
        if let Some(id) = uri.strip_prefix("domain://docs/")
            && let Some(doc) = FIXTURE_DOCS.doc(id)
        {
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                doc.body, uri,
            )]));
        }
        if uri == "domain://contract" {
            let declaration =
                ContractDeclaration::from_docs(&FIXTURE_DOCS, CapabilityInventory::default());
            let text = serde_json::to_string(&declaration).expect("declaration serializes");
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text, uri,
            )]));
        }
        Err(rmcp::ErrorData::invalid_params("unknown resource", None))
    }
}

#[tokio::test]
async fn certifies_a_domain_without_linking_its_implementation() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let service: StreamableHttpService<DomainFixture, LocalSessionManager> =
        StreamableHttpService::new(
            || Ok(DomainFixture),
            LocalSessionManager::default().into(),
            veoveo_mcp_contract::canonical_streamable_http_server_config(),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let admin = Router::new()
        .route(
            "/domain/admin/docs/llms.txt",
            axum::routing::get(|| async { FIXTURE_DOCS.llms_txt() }),
        )
        .route(
            "/domain/admin/docs/{id}",
            axum::routing::get(
                |axum::extract::Path(id): axum::extract::Path<String>| async move {
                    match FIXTURE_DOCS.doc(&id) {
                        Some(doc) => (axum::http::StatusCode::OK, doc.body.to_owned()),
                        None => (axum::http::StatusCode::NOT_FOUND, String::new()),
                    }
                },
            ),
        );
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .nest_service("/domain/mcp", service)
                .merge(admin),
        )
        .await
    });

    let profile = HostedServerConformanceProfile {
        schema_version: HostedServerProfileSchema::V1,
        profile_id: "anonymous-extension".to_owned(),
        contract_revision: "veoveo.io/hosted-mcp/v1".to_owned(),
        endpoint: format!("http://{address}/domain/mcp"),
        server_slug: "domain".to_owned(),
        owned_resource_schemes: BTreeSet::from(["domain".to_owned()]),
        http: HttpBoundaryProfile {
            require_authentication_rejection: false,
            rejected_host: None,
            health_url: None,
            readiness_url: None,
            docs_llms_url: Some(format!("http://{address}/domain/admin/docs/llms.txt")),
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
        run_hosted_server_conformance(&profile, &ConformanceCredentials::default()).await?;
    assert!(report.passed(), "{:#?}", report.checks);
    assert_eq!(
        report
            .implementation
            .as_ref()
            .map(|value| value.name.as_str()),
        Some("domain")
    );

    server.abort();
    Ok(())
}
