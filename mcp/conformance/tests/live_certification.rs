use std::collections::BTreeSet;

use axum::Router;
use rmcp::{
    RoleServer, ServerHandler,
    model::{
        Implementation, JsonObject, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        Resource, ServerCapabilities, ServerInfo, Tool,
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
            resources: vec![Resource::new("domain://docs", "contract documentation")],
            next_cursor: None,
            meta: None,
        })
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
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().nest_service("/domain/mcp", service)).await
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
