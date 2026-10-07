use std::{collections::BTreeSet, sync::LazyLock};

use rmcp::model::{Implementation, Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceTemplateUri, ResourceUri, ScopeName};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
enum Permission {
    #[vocabulary(rename = "independent:read")]
    Read,
    #[vocabulary(rename = "independent:write")]
    Write,
}

// The fixture deliberately includes a broken codec to exercise admission failures.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Address {
    Docs,
    Agents,
    Design,
    Contract,
    BrokenRoundTrip,
}
impl ResourceAddress for Address {
    type Error = veoveo_types::IdentifierError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        match uri.as_str() {
            "independent://docs" => Ok(Self::Docs),
            "independent://docs/agents" => Ok(Self::Agents),
            "independent://docs/design" => Ok(Self::Design),
            "independent://contract" => Ok(Self::Contract),
            _ => Err(veoveo_types::IdentifierError::new(
                uri.as_str(),
                "unknown fixture address",
            )),
        }
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(ResourceUri::new(match self {
            Self::Docs | Self::BrokenRoundTrip => "independent://docs",
            Self::Agents => "independent://docs/agents",
            Self::Design => "independent://docs/design",
            Self::Contract => "independent://contract",
        })
        .unwrap())
    }
}

fn documents(owner: &'static str) -> ServerDocs {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/compliance-example.json")).unwrap();
    value["server"] = serde_json::json!(owner);
    let profile: veoveo_mcp_contract::docs::ComplianceProfile =
        serde_json::from_value(value).unwrap();
    let manual = format!(
        "# Manual\n\n## Contract Compliance\n\n{}\n",
        veoveo_mcp_contract::docs::render_compliance(&profile)
    );
    ServerDocs::new(owner)
        .with_doc("agents", "Agent manual", Box::leak(manual.into_boxed_str()))
        .with_doc("design", "Design", "# Design")
        .with_profile_json(&serde_json::to_string(&profile).unwrap())
        .unwrap()
}

static DOCUMENTS: LazyLock<ServerDocs> = LazyLock::new(|| documents("independent"));
static FOREIGN_DOCUMENTS: LazyLock<ServerDocs> = LazyLock::new(|| documents("foreign"));
static EMPTY_DOCUMENTS: LazyLock<ServerDocs> = LazyLock::new(|| ServerDocs::new("independent"));
static DUPLICATE_DOCUMENTS: LazyLock<ServerDocs> = LazyLock::new(|| {
    documents("independent").with_doc("agents", "Duplicate manual", "# Duplicate")
});
static BLANK_DOCUMENT: LazyLock<ServerDocs> =
    LazyLock::new(|| documents("independent").with_doc("extra", "Empty", "  "));

const VALID: u8 = 0;
const WRONG_NAME: u8 = 1;
const WRONG_OWNER: u8 = 2;
const NO_RESOURCES: u8 = 3;
const NO_DOCUMENTS: u8 = 4;
const DUPLICATE_DOCUMENT: u8 = 5;
const DUPLICATE_RESOURCE: u8 = 6;
const MISSING_ROOT: u8 = 7;
const MISSING_DOCUMENT_RESOURCE: u8 = 8;
const DUPLICATE_SCOPE: u8 = 9;
const EMPTY_SCOPES: u8 = 10;
const BAD_TEMPLATE: u8 = 11;
const DUPLICATE_TEMPLATE: u8 = 12;
const EMPTY_DOCUMENT_BODY: u8 = 13;
const NO_COMPLETIONS: u8 = 14;
struct Fixture<const CASE: u8>;

impl<const CASE: u8> McpServerContract for Fixture<CASE> {
    type Scope = Permission;
    type Resource = Address;
    fn slug() -> ServerSlug {
        ServerSlug::parse("independent").unwrap()
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::parse("independent").unwrap()
    }
    fn scopes() -> &'static [Permission] {
        match CASE {
            DUPLICATE_SCOPE => &[Permission::Read, Permission::Read],
            EMPTY_SCOPES => &[],
            _ => &[Permission::Read],
        }
    }
    fn documents() -> &'static ServerDocs {
        match CASE {
            WRONG_OWNER => &FOREIGN_DOCUMENTS,
            NO_DOCUMENTS => &EMPTY_DOCUMENTS,
            DUPLICATE_DOCUMENT => &DUPLICATE_DOCUMENTS,
            EMPTY_DOCUMENT_BODY => &BLANK_DOCUMENT,
            _ => &DOCUMENTS,
        }
    }
    fn server_config() -> ServerConfig {
        let mut config = ServerConfig::default();
        config.server_info = Implementation::new(
            if CASE == WRONG_NAME {
                "foreign"
            } else {
                "independent"
            },
            "1.0.0",
        );
        config.capabilities = match CASE {
            NO_RESOURCES => ServerCapabilities::builder().enable_completions().build(),
            NO_COMPLETIONS => ServerCapabilities::builder().enable_resources().build(),
            _ => ServerCapabilities::builder()
                .enable_resources()
                .enable_completions()
                .build(),
        };
        config
    }
    fn resources() -> Result<Vec<McpResource<Address>>, McpSetupError> {
        let mut addresses = vec![
            Address::Docs,
            Address::Agents,
            Address::Design,
            Address::Contract,
        ];
        match CASE {
            DUPLICATE_RESOURCE => addresses.push(Address::Docs),
            MISSING_ROOT => addresses.retain(|address| *address != Address::Contract),
            MISSING_DOCUMENT_RESOURCE => addresses.retain(|address| *address != Address::Design),
            _ => {}
        }
        addresses
            .into_iter()
            .map(|address| {
                McpResource::new(address, |uri| {
                    Resource::new(uri, "Fixture").with_mime_type("text/plain")
                })
            })
            .collect()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        let text = if CASE == BAD_TEMPLATE {
            "relative/{id}"
        } else {
            "independent://item/{+id}{?cursor}"
        };
        let template =
            ResourceTemplateUri::new(text).map_err(|_| McpSetupError::InvalidTemplate)?;
        let descriptor =
            McpResourceTemplate::new(template, |uri| ResourceTemplate::new(uri, "Item"))?;
        let mut templates = vec![descriptor; if CASE == DUPLICATE_TEMPLATE { 2 } else { 1 }];
        templates.push(McpResourceTemplate::new(
            ResourceTemplateUri::new("independent://docs/{doc_id}").unwrap(),
            |uri| ResourceTemplate::new(uri, "Document"),
        )?);
        Ok(templates)
    }
}

fn rejects<const CASE: u8>(expected: McpSetupError) {
    assert_eq!(McpServerSetup::<Fixture<CASE>>::new().err(), Some(expected));
}

#[test]
fn scope_membership_keeps_unknown_grants_and_rejects_undeclared_permissions() {
    let setup = McpServerSetup::<Fixture<VALID>>::new().unwrap();
    let grants = BTreeSet::from([
        Permission::Read.into(),
        Permission::Write.into(),
        ScopeName::parse("another:read").unwrap(),
    ]);
    assert!(setup.has_scope(&grants, Permission::Read));
    assert!(!setup.has_scope(&grants, Permission::Write));
    assert!(!setup.has_scope(&BTreeSet::new(), Permission::Read));
    assert_eq!(
        setup.scope_names(),
        &BTreeSet::from([Permission::Read.into()])
    );
    rejects::<DUPLICATE_SCOPE>(McpSetupError::DuplicateScope);
}

#[test]
fn servers_without_domain_scopes_have_an_empty_vocabulary() {
    let setup = McpServerSetup::<Fixture<EMPTY_SCOPES>>::new().unwrap();
    assert!(setup.scope_names().is_empty());
    assert!(!setup.has_scope(&BTreeSet::from([Permission::Read.into()]), Permission::Read));
}

#[test]
fn setup_preserves_typed_addresses_metadata_and_rfc6570_declarations() {
    let setup = McpServerSetup::<Fixture<VALID>>::new().unwrap();
    let resources = setup.resources();
    assert_eq!(resources.len(), 4);
    assert!(
        resources
            .windows(2)
            .all(|pair| pair[0].descriptor().uri < pair[1].descriptor().uri)
    );
    for resource in resources {
        assert_eq!(
            resource.address().to_uri().unwrap().as_str(),
            resource.descriptor().uri
        );
        assert_eq!(
            resource.descriptor().mime_type.as_deref(),
            Some("text/plain")
        );
    }
    assert_eq!(
        setup
            .resource_templates()
            .iter()
            .find(|t| t.descriptor().name == "Item")
            .unwrap()
            .template()
            .as_str(),
        "independent://item/{+id}{?cursor}"
    );
    assert_eq!(
        setup.documents().server(),
        setup.server_config().server_info.name
    );
}

#[test]
fn descriptor_builder_cannot_override_address_or_hide_a_broken_codec() {
    assert_eq!(
        McpResource::new(Address::Docs, |_| Resource::new("foreign://docs", "Docs")).unwrap_err(),
        McpSetupError::DescriptorAddressMismatch
    );
    assert_eq!(
        McpResource::new(Address::BrokenRoundTrip, |uri| Resource::new(uri, "Docs")).unwrap_err(),
        McpSetupError::ResourceRoundTrip
    );
    assert_eq!(
        McpResource::new(Address::Docs, |uri| Resource::new(uri, " ")).unwrap_err(),
        McpSetupError::MissingResourceName
    );
}

#[test]
fn inconsistent_server_identity_and_missing_capability_fail_before_serving() {
    rejects::<WRONG_NAME>(McpSetupError::IdentityMismatch);
    rejects::<WRONG_OWNER>(McpSetupError::IdentityMismatch);
    rejects::<NO_RESOURCES>(McpSetupError::MissingResourcesCapability);
    rejects::<NO_COMPLETIONS>(McpSetupError::MissingCompletionsCapability);
}

#[test]
fn document_and_discovery_coverage_is_checked_before_serving() {
    rejects::<NO_DOCUMENTS>(McpSetupError::MissingDocument);
    rejects::<EMPTY_DOCUMENT_BODY>(McpSetupError::MissingDocument);
    rejects::<DUPLICATE_DOCUMENT>(McpSetupError::InvalidDocument);
    rejects::<DUPLICATE_RESOURCE>(McpSetupError::DuplicateResource);
    rejects::<MISSING_ROOT>(McpSetupError::MissingWellKnownResource);
    rejects::<MISSING_DOCUMENT_RESOURCE>(McpSetupError::MissingWellKnownResource);
}

#[test]
fn invalid_or_duplicate_template_declarations_are_rejected() {
    rejects::<BAD_TEMPLATE>(McpSetupError::InvalidTemplate);
    rejects::<DUPLICATE_TEMPLATE>(McpSetupError::DuplicateTemplate);
}

#[test]
fn template_metadata_cannot_override_the_validated_reference() {
    let template = ResourceTemplateUri::new("independent://item/{id}").unwrap();
    assert_eq!(
        McpResourceTemplate::new(template.clone(), |_| ResourceTemplate::new(
            "foreign://{id}",
            "Other"
        ))
        .unwrap_err(),
        McpSetupError::DescriptorTemplateMismatch
    );
    assert_eq!(
        McpResourceTemplate::new(template, |uri| ResourceTemplate::new(uri, " ")).unwrap_err(),
        McpSetupError::InvalidTemplate
    );
}

#[cfg(feature = "testing")]
mod product_results {
    use super::*;
    use rmcp::{
        ErrorData, RoleServer,
        handler::server::{router::tool::ToolRouter, wrapper::Parameters},
        model::{CallToolResult, ReadResourceRequestParams},
        service::RequestContext,
    };
    use veoveo_mcp_contract::hosting::{
        DomainRead, DomainServer, Hosted, product_result,
        testing::{self, TestGateway},
    };

    #[derive(Clone)]
    struct ProductDomain {
        router: ToolRouter<Self>,
    }

    #[derive(serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Request {
        case: Case,
    }

    #[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
    enum Case {
        Current,
        Retired,
        Mixed,
        Mismatch,
        Missing,
        WrongType,
        Null,
        InvalidUri,
    }

    #[rmcp::tool_router]
    impl ProductDomain {
        #[rmcp::tool(
            name = "produce_product",
            description = "Contract fixture product result"
        )]
        async fn produce(
            &self,
            Parameters(request): Parameters<Request>,
        ) -> Result<CallToolResult, ErrorData> {
            let output = match request.case {
                Case::Current => {
                    #[derive(serde::Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Product {
                        result_uri: ResourceUri,
                    }
                    serde_json::to_value(Product {
                        result_uri: ResourceUri::new("independent://products/1").unwrap(),
                    })
                    .unwrap()
                }
                Case::Retired => serde_json::json!({"result_uri":"independent://products/1"}),
                Case::Mixed => {
                    serde_json::json!({"resultUri":"independent://products/1", "result_uri":"independent://products/1"})
                }
                Case::Mismatch => serde_json::json!({"resultUri":"independent://products/2"}),
                Case::Missing => serde_json::json!({}),
                Case::WrongType => serde_json::json!({"resultUri":42}),
                Case::Null => serde_json::json!({"resultUri":null}),
                Case::InvalidUri => serde_json::json!({"resultUri":"not a resource URI"}),
            };
            product_result(
                "Product ready",
                Resource::new("independent://products/1", "Product"),
                &output,
            )
        }
    }

    impl DomainServer for ProductDomain {
        type Contract = Fixture<VALID>;
        fn setup() -> &'static McpServerSetup<Self::Contract> {
            static SETUP: LazyLock<McpServerSetup<Fixture<VALID>>> =
                LazyLock::new(|| McpServerSetup::new().unwrap());
            &SETUP
        }
        fn tool_router(&self) -> &ToolRouter<Self> {
            &self.router
        }
        async fn read(
            &self,
            _: Address,
            _: &ReadResourceRequestParams,
            _: &RequestContext<RoleServer>,
        ) -> Result<DomainRead, ErrorData> {
            Err(ErrorData::resource_not_found(
                "No fixture product read",
                None,
            ))
        }
    }

    #[tokio::test]
    async fn gateway_product_results_require_only_current_result_uri_and_matching_link() {
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            let gateway = TestGateway::new(testing::for_domain::<ProductDomain>()
                .handler(|| Hosted::new(ProductDomain { router: ProductDomain::tool_router() })).build());
            let bearer = gateway.token();
            let mut violations = Vec::new();
            for (case, succeeds) in [("current", true), ("retired", false), ("mixed", false), ("mismatch", false), ("missing", false), ("wrong_type", false), ("null", false), ("invalid_uri", false)] {
                let request = serde_json::json!({"name":"produce_product", "arguments":{"case":case}});
                let (status, response) = gateway.rpc_with("tools/call", request.clone(), Some(&bearer)).await;
                let success = response.get("result").is_some_and(|result| result.get("isError") != Some(&serde_json::Value::Bool(true)));
                let intended_error = response["error"]["code"] == -32603
                    && response["error"]["message"].as_str().is_some_and(|message| {
                        message.starts_with("product result") && message.ends_with("must match its resource link")
                    });
                if status.as_u16() != 200 || (succeeds && !success) || (!succeeds && !intended_error) {
                    violations.push(serde_json::json!({"request":request,"httpStatus":status.as_u16(),"response":response}));
                } else if succeeds {
                    assert_eq!(response["result"]["structuredContent"]["resultUri"], "independent://products/1");
                    assert_eq!(response["result"]["content"][1]["type"], "resource_link");
                    assert_eq!(response["result"]["content"][1]["uri"], "independent://products/1");
                    assert!(response["result"]["structuredContent"].get("result_uri").is_none());
                }
            }
            assert!(violations.is_empty(), "Product result contract violations: {}", serde_json::Value::Array(violations));
        }).await.expect("Gateway product result matrix exceeded twenty seconds");
    }

    #[tokio::test]
    async fn gateway_traces_preserve_routing_without_query_or_credentials() {
        use std::{
            io::Write,
            sync::{Arc, Mutex},
        };
        use tracing::instrument::WithSubscriber;
        struct TraceWriter(Arc<Mutex<Vec<u8>>>);
        impl Write for TraceWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            let captured = Arc::new(Mutex::new(Vec::new()));
            let writer = captured.clone();
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE).with_ansi(false).without_time()
                .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NEW)
                .with_writer(move || TraceWriter(writer.clone())).finish();
            let gateway = TestGateway::new(testing::for_domain::<ProductDomain>()
                .handler(|| Hosted::new(ProductDomain { router: ProductDomain::tool_router() }))
                .public_routes(axum::Router::new().route("/trace-query", axum::routing::get(|uri: axum::http::Uri| async move {
                    uri.query().unwrap_or_default().to_owned()
                }))).build());
            let query = "veoveo_trace_query_sentinel=veoveo_trace_query_value&access_token=synthetic-trace-query-token&signature=synthetic-trace-signature%2Fvalue";
            let request = gateway.request(&format!("/trace-query?{query}"))
                .method("GET").version(axum::http::Version::HTTP_11)
                .header("authorization", "Bearer synthetic-trace-header-token")
                .header("cookie", "fixture=synthetic-trace-cookie")
                .body(axum::body::Body::empty()).unwrap();
            let expected_path = request.uri().path().to_owned();
            let (status, body) = gateway.send(request).with_subscriber(subscriber).await;
            assert_eq!(status.as_u16(), 200);
            assert_eq!(body, query, "the route must receive the original query");
            let trace = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
            assert!(trace.contains("method=GET"), "missing request method");
            assert!(trace.contains(&expected_path), "missing request path");
            assert!(trace.contains("version=HTTP/1.1"), "missing request version");
            let forbidden = ["veoveo_trace_query_sentinel", "veoveo_trace_query_value", "access_token", "synthetic-trace-query-token", "synthetic-trace-signature", "synthetic-trace-header-token", "synthetic-trace-cookie"]
                .into_iter().filter(|value| trace.contains(value)).collect::<Vec<_>>();
            assert!(forbidden.is_empty(), "Request trace contains synthetic query/credential sentinels: {forbidden:?}");
        }).await.expect("Gateway trace query control exceeded twenty seconds");
    }
}
