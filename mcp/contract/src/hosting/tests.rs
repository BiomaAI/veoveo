//! In-process tests of the hosted server shape, with real gateway tokens.

use std::sync::LazyLock;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{TimeDelta, Utc};
use rmcp::{
    ErrorData, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
    tool, tool_router,
};
use tower::ServiceExt;
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceTemplateUri, ResourceUri};

use super::{
    DomainAddress, DomainRead, DomainServer, Hosted, HostedServer, gateway_identity, served_by_host,
};
use crate::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenIssuer, GatewayProfileId, PublicDeployment,
    ServerSlug, TokenIssuer,
    docs::ServerDocs,
    internal_auth::tests::{authority, principal, signing_key, trust_bundle},
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};

veoveo_types::scope_enum! {
    enum FixtureScope {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FixtureResource {
    Docs,
    Document(&'static str),
    Contract,
    Items,
    Volatile,
}

impl ResourceAddress for FixtureResource {
    type Error = veoveo_types::IdentifierError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        match uri.as_str() {
            "fixture://docs" => Ok(Self::Docs),
            "fixture://docs/agents" => Ok(Self::Document("agents")),
            "fixture://docs/design" => Ok(Self::Document("design")),
            "fixture://contract" => Ok(Self::Contract),
            "fixture://items" => Ok(Self::Items),
            "fixture://volatile" => Ok(Self::Volatile),
            _ => Err(veoveo_types::IdentifierError::new(
                uri.as_str(),
                "unknown fixture address",
            )),
        }
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        ResourceUri::new(match self {
            Self::Docs => "fixture://docs".to_owned(),
            Self::Document(id) => format!("fixture://docs/{id}"),
            Self::Contract => "fixture://contract".to_owned(),
            Self::Items => "fixture://items".to_owned(),
            Self::Volatile => "fixture://volatile".to_owned(),
        })
        .map_err(|_| veoveo_types::IdentifierError::new("fixture", "invalid fixture address"))
    }
}

static DOCS: LazyLock<ServerDocs> = LazyLock::new(|| {
    ServerDocs::new("fixture")
        .with_doc("agents", "Agent manual", "# Manual")
        .with_doc("design", "Design", "# Design")
});

struct FixtureContract;

impl McpServerContract for FixtureContract {
    type Scope = FixtureScope;
    type Resource = FixtureResource;
    fn slug() -> ServerSlug {
        ServerSlug::new("fixture").unwrap()
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::new("fixture").unwrap()
    }
    fn scopes() -> &'static [FixtureScope] {
        FixtureScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &DOCS
    }
    fn server_config() -> ServerConfig {
        let mut config = ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        );
        config.server_info = rmcp::model::Implementation::new("fixture", "1");
        config
    }
    fn resources() -> Result<Vec<McpResource<FixtureResource>>, McpSetupError> {
        [
            (FixtureResource::Docs, "docs"),
            (FixtureResource::Document("agents"), "agents"),
            (FixtureResource::Document("design"), "design"),
            (FixtureResource::Contract, "contract"),
            (FixtureResource::Items, "items"),
            (FixtureResource::Volatile, "volatile"),
        ]
        .into_iter()
        .map(|(address, name)| McpResource::new(address, |uri| Resource::new(uri, name)))
        .collect()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        let docs = veoveo_mcp_knowledge_extension::docs::member_template(&Self::scheme());
        vec![McpResourceTemplate::new(docs, |uri| {
            ResourceTemplate::new(uri, "doc")
        })]
        .into_iter()
        .chain(std::iter::once(McpResourceTemplate::new(
            ResourceTemplateUri::new("fixture://items/{item_id}").unwrap(),
            |uri| ResourceTemplate::new(uri, "item"),
        )))
        .collect()
    }
}

static SETUP: LazyLock<McpServerSetup<FixtureContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("fixture setup"));

#[derive(Clone)]
struct FixtureDomain {
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EchoRequest {
    text: String,
}

#[tool_router]
impl FixtureDomain {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Echo text back to the verified caller.")]
    async fn echo(
        &self,
        Parameters(request): Parameters<EchoRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let identity = gateway_identity(&context)?;
        Ok(CallToolResult::success(vec![
            rmcp::model::ContentBlock::text(format!(
                "{} from {}",
                request.text, identity.actor.subject
            )),
        ]))
    }
}

impl DomainServer for FixtureDomain {
    type Contract = FixtureContract;
    fn setup() -> &'static McpServerSetup<FixtureContract> {
        &SETUP
    }
    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }
    fn prompts(&self) -> Vec<rmcp::model::Prompt> {
        vec![rmcp::model::Prompt::new(
            "summarize",
            Some("Summarize the items."),
            None,
        )]
    }
    async fn get_prompt(
        &self,
        request: rmcp::model::GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResult, ErrorData> {
        if request.name != "summarize" {
            return Err(super::unknown_prompt(&request.name));
        }
        Ok(rmcp::model::GetPromptResult::new(vec![
            rmcp::model::PromptMessage::new_text(
                rmcp::model::Role::User,
                "Summarize fixture://items.",
            ),
        ]))
    }
    async fn read(
        &self,
        address: DomainAddress<FixtureContract>,
        request: &ReadResourceRequestParams,
        _context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, ErrorData> {
        match address {
            FixtureResource::Items => Ok(DomainRead::private(ReadResourceResult::new(vec![
                ResourceContents::text("[]", &request.uri),
            ]))),
            FixtureResource::Volatile => Ok(DomainRead::no_store(ReadResourceResult::new(vec![
                ResourceContents::text("{}", &request.uri),
            ]))),
            FixtureResource::Docs | FixtureResource::Document(_) | FixtureResource::Contract => {
                Err(served_by_host())
            }
        }
    }
}

fn router() -> Router {
    let deployment = PublicDeployment::new("https://veoveo.example").unwrap();
    HostedServer::for_domain::<FixtureDomain>()
        .deployment(&deployment, false)
        .unwrap()
        .internal_trust(trust_bundle("k1"))
        .unwrap()
        .handler(|| Hosted::new(FixtureDomain::new()))
        .readiness(|| async { true })
        .build()
        .into_router()
}

fn token() -> String {
    GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
        signing_key("k1"),
    )
    .issue(
        GatewayProfileId::new("operations").unwrap(),
        ServerSlug::new("fixture").unwrap(),
        principal(),
        authority(),
        None,
        Utc::now() + TimeDelta::minutes(5),
    )
    .unwrap()
    .bearer_token
}

async fn send(request: Request<Body>) -> (StatusCode, String) {
    let response = router().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

fn get(path: &str) -> axum::http::request::Builder {
    Request::builder()
        .uri(path)
        .header("host", "veoveo.example")
}

async fn rpc(
    method: &str,
    params: serde_json::Value,
    bearer: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut params = params;
    params["_meta"] = serde_json::json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "hosting-test", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    let mut request = Request::builder()
        .method("POST")
        .uri("/fixture/mcp")
        .header("host", "veoveo.example")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2026-07-28")
        .header("mcp-method", method);
    if let Some(bearer) = bearer {
        request = request.header("authorization", format!("Bearer {bearer}"));
    }
    if let Some(name) = params["name"].as_str().or(params["uri"].as_str()) {
        request = request.header("mcp-name", name);
    }
    let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let (status, text) = send(request.body(Body::from(body.to_string())).unwrap()).await;
    (
        status,
        serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn health_and_readiness_need_no_authentication() {
    let (status, body) = send(get("/fixture/healthz").body(Body::empty()).unwrap()).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "ok"));
    let (status, _) = send(get("/fixture/readyz").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn hosts_are_validated_before_routing() {
    let (status, _) = send(
        Request::builder()
            .uri("/fixture/healthz")
            .header("host", "attacker.example")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::MISDIRECTED_REQUEST);
    let (status, _) = send(
        Request::builder()
            .uri("/fixture/healthz")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn mcp_and_admin_routes_require_a_gateway_token() {
    let (status, _) = rpc("resources/list", serde_json::json!({}), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = rpc("resources/list", serde_json::json!({}), Some("not-a-token")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        get("/fixture/admin/docs/llms.txt")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = send(
        get("/fixture/admin/docs/llms.txt")
            .header("authorization", format!("Bearer {}", token()))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("(agents)"));
}

#[tokio::test]
async fn discovery_comes_from_the_checked_setup() {
    let token = token();
    let (_, body) = rpc("resources/list", serde_json::json!({}), Some(&token)).await;
    let uris: Vec<_> = body["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|resource| resource["uri"].as_str().unwrap().to_owned())
        .collect();
    assert!(uris.contains(&"fixture://items".to_owned()));
    assert!(uris.contains(&"fixture://contract".to_owned()));
    assert_eq!(body["result"]["cacheScope"], "private");

    let (_, body) = rpc(
        "resources/templates/list",
        serde_json::json!({}),
        Some(&token),
    )
    .await;
    assert_eq!(
        body["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let (_, body) = rpc("tools/list", serde_json::json!({}), Some(&token)).await;
    assert_eq!(body["result"]["tools"][0]["name"], "echo");
}

#[tokio::test]
async fn the_host_serves_contract_and_admits_domain_addresses() {
    let token = token();
    let (_, body) = rpc(
        "resources/read",
        serde_json::json!({"uri": "fixture://contract"}),
        Some(&token),
    )
    .await;
    let contract: serde_json::Value =
        serde_json::from_str(body["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(contract["server"], "fixture");

    let (_, body) = rpc(
        "resources/read",
        serde_json::json!({"uri": "fixture://items"}),
        Some(&token),
    )
    .await;
    assert_eq!(body["result"]["contents"][0]["text"], "[]");
    assert_eq!(body["result"]["cacheScope"], "private");

    let (_, body) = rpc(
        "resources/read",
        serde_json::json!({"uri": "fixture://unknown"}),
        Some(&token),
    )
    .await;
    assert_eq!(body["error"]["code"], -32602);
}

#[tokio::test]
async fn tools_receive_the_verified_caller() {
    let (_, body) = rpc(
        "tools/call",
        serde_json::json!({"name": "echo", "arguments": {"text": "hello"}}),
        Some(&token()),
    )
    .await;
    assert_eq!(body["result"]["content"][0]["text"], "hello from user-1");
}

#[tokio::test]
async fn each_read_keeps_the_cache_policy_its_domain_chose() {
    let token = token();
    let (_, body) = rpc(
        "resources/read",
        serde_json::json!({"uri": "fixture://items"}),
        Some(&token),
    )
    .await;
    assert_eq!(body["result"]["ttlMs"], crate::PRIVATE_RESOURCE_TTL_MS);
    let (_, body) = rpc(
        "resources/read",
        serde_json::json!({"uri": "fixture://volatile"}),
        Some(&token),
    )
    .await;
    assert_eq!(body["result"]["ttlMs"], 0);
    assert_eq!(body["result"]["cacheScope"], "private");
}

#[tokio::test]
async fn prompts_come_from_the_domain() {
    let token = token();
    let (_, body) = rpc("prompts/list", serde_json::json!({}), Some(&token)).await;
    assert_eq!(body["result"]["prompts"][0]["name"], "summarize");
    let (_, body) = rpc(
        "prompts/get",
        serde_json::json!({"name": "summarize"}),
        Some(&token),
    )
    .await;
    assert_eq!(
        body["result"]["messages"][0]["content"]["text"],
        "Summarize fixture://items."
    );
    let (_, body) = rpc(
        "prompts/get",
        serde_json::json!({"name": "unknown"}),
        Some(&token),
    )
    .await;
    assert_eq!(body["error"]["code"], -32602);
}

#[tokio::test]
async fn the_host_completes_document_ids() {
    let (_, body) = rpc(
        "completion/complete",
        serde_json::json!({
            "ref": {"type": "ref/resource", "uri": "fixture://docs/{doc_id}"},
            "argument": {"name": "doc_id", "value": "de"}
        }),
        Some(&token()),
    )
    .await;
    assert_eq!(
        body["result"]["completion"]["values"],
        serde_json::json!(["design"])
    );
}
