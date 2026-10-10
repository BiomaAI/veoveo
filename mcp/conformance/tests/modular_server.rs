use std::{collections::BTreeSet, time::Duration};

use axum::{Router, extract::Request, http::StatusCode, middleware::Next, response::IntoResponse};
use rmcp::{
    ClientLifecycleMode, ClientServiceExt,
    model::{ReadResourceRequestParams, ResourceContents},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
        streamable_http_server::StreamableHttpService,
    },
};
use veoveo_mcp_conformance::{
    ConformanceCredentials, HostedServerConformanceProfile, HostedServerProfileSchema,
    HttpBoundaryProfile, SurfaceExpectation, SurfaceProfile,
    run_hosted_server_conformance_with_evidence,
};
use veoveo_modular_fixture_mcp::{
    contract::{ObservatoryResource, ObservatoryScope, Reading, ReadingId},
    mcp::{FixtureGrants, ObservatoryMcp, SETUP},
};
use veoveo_types::{ResourceAddress, ScopeName};

struct OwnedServer(Option<tokio::task::JoinHandle<()>>);
impl Drop for OwnedServer {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}
impl OwnedServer {
    async fn stop(mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

async fn authenticate(mut request: Request, next: Next) -> axum::response::Response {
    let grants = match request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        Some("Bearer fixture-read") => BTreeSet::from([
            ObservatoryScope::Read.into(),
            ScopeName::parse("unrelated:custom").unwrap(),
        ]),
        Some("Bearer fixture-unrelated") => {
            BTreeSet::from([ScopeName::parse("unrelated:custom").unwrap()])
        }
        _ => return StatusCode::UNAUTHORIZED.into_response(),
    };
    request.extensions_mut().insert(FixtureGrants(grants));
    next.run(request).await
}

#[tokio::test]
async fn independent_typed_server_passes_hosted_conformance_and_scope_denial() -> anyhow::Result<()>
{
    tokio::time::timeout(Duration::from_secs(60), qualify()).await??;
    Ok(())
}

async fn qualify() -> anyhow::Result<()> {
    std::sync::LazyLock::force(&SETUP);
    let catalog_fault = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    let factory_fault = catalog_fault.clone();
    let service = StreamableHttpService::new(
        move || Ok(CatalogFixture(factory_fault.clone())),
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config(),
    );
    let router = Router::new()
        .nest_service("/observatory/mcp", service)
        .route(
            "/observatory/admin/docs/llms.txt",
            axum::routing::get(
                |axum::Extension(grants): axum::Extension<FixtureGrants>| async move {
                    if !SETUP.has_scope(&grants.0, ObservatoryScope::Read) {
                        return (StatusCode::FORBIDDEN, String::new());
                    }
                    (StatusCode::OK, SETUP.documents().llms_txt())
                },
            ),
        )
        .route(
            "/observatory/admin/docs/{id}",
            axum::routing::get(
                |axum::extract::Path(id): axum::extract::Path<String>,
                 axum::Extension(grants): axum::Extension<FixtureGrants>| async move {
                    if !SETUP.has_scope(&grants.0, ObservatoryScope::Read) {
                        return (StatusCode::FORBIDDEN, String::new());
                    }
                    match SETUP.documents().doc(&id) {
                        Some(doc) => (StatusCode::OK, doc.body.to_owned()),
                        None => (StatusCode::NOT_FOUND, String::new()),
                    }
                },
            ),
        )
        .layer(axum::middleware::from_fn(authenticate));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = OwnedServer(Some(tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("owned fixture listener");
    })));
    let endpoint = format!("http://{address}/observatory/mcp");
    let profile = HostedServerConformanceProfile {
        schema_version: HostedServerProfileSchema::V2,
        profile_id: "modular-fixture".into(),
        contract_revision: veoveo_mcp_contract::HOSTED_MCP_CONTRACT_REVISION.into(),
        endpoint: endpoint.clone(),
        server_slug: "observatory".into(),
        owned_resource_schemes: BTreeSet::from(["observatory".into()]),
        http: HttpBoundaryProfile {
            require_authentication_rejection: true,
            rejected_host: None,
            health_url: None,
            readiness_url: None,
            docs_llms_url: format!("http://{address}/observatory/admin/docs/llms.txt"),
        },
        surfaces: SurfaceProfile {
            tools: SurfaceExpectation::Forbidden,
            resources: SurfaceExpectation::Required,
            resource_templates: SurfaceExpectation::Required,
            prompts: SurfaceExpectation::Forbidden,
            completions: SurfaceExpectation::Required,
            tasks: SurfaceExpectation::Forbidden,
            subscriptions: SurfaceExpectation::Forbidden,
            required_tools: BTreeSet::new(),
            required_resources: BTreeSet::from(["observatory://readings".into()]),
            required_resource_templates: BTreeSet::from([
                "observatory://reading/{reading_id}".into()
            ]),
            required_prompts: BTreeSet::new(),
        },
    };
    // The same source checks run through a combined endpoint without pretending
    // that its implementation identity is the selected source's identity.
    for route in [
        veoveo_mcp_conformance::KnowledgeRoute::Direct,
        veoveo_mcp_conformance::KnowledgeRoute::Gateway,
    ] {
        let target = veoveo_mcp_conformance::KnowledgeSourceTarget::new(
            endpoint.parse()?,
            "observatory".parse()?,
            ["observatory".parse()?].into(),
            route,
        )?;
        let source = veoveo_mcp_conformance::run_knowledge_source_conformance(
            &target,
            &ConformanceCredentials::bearer("fixture-read"),
            &Default::default(),
        )
        .await?;
        assert!(source.passed(), "{:#?}", source.checks);
        assert!(
            source
                .checks
                .iter()
                .all(|c| c.requirement_id.starts_with('K'))
        );
    }
    qualify_selected_catalog(&endpoint, &catalog_fault, &profile).await?;
    let missing = veoveo_mcp_conformance::KnowledgeSourceTarget::new(
        endpoint.parse()?,
        "absent".parse()?,
        ["absent".parse()?].into(),
        veoveo_mcp_conformance::KnowledgeRoute::Gateway,
    )?;
    let source = veoveo_mcp_conformance::run_knowledge_source_conformance(
        &missing,
        &ConformanceCredentials::bearer("fixture-read"),
        &Default::default(),
    )
    .await?;
    assert!(
        !source.passed(),
        "an absent source cannot pass on an empty selection"
    );
    check_source_cli(&endpoint).await?;

    let resource = ObservatoryResource::Reading(ReadingId::new("sensor-a")?).to_uri()?;
    let mut document_revision = None;
    let mut observed_reading = None;
    for (token, allowed) in [("fixture-read", true), ("fixture-unrelated", false)] {
        let client = ()
            .serve_with_lifecycle(
                StreamableHttpClientTransport::from_config(
                    StreamableHttpClientTransportConfig::with_uri(endpoint.clone())
                        .auth_header(token),
                ),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                },
            )
            .await?;
        use veoveo_mcp_contract::docs::knowledge_extension as knowledge;
        let document_uri = veoveo_types::ResourceUri::new("observatory://docs/design")?;
        if allowed {
            let plain = client
                .read_resource(ReadResourceRequestParams::new(document_uri.as_str()))
                .await?;
            assert!(knowledge::client::observation(&plain)?.is_none());
            let full = knowledge_read(&client, &document_uri, None).await?;
            let observation = knowledge::client::validate_read(&full, &document_uri, None)?
                .expect("negotiated docs observation");
            document_revision = Some(observation.revision().clone());
        }
        let conditional = knowledge_read(&client, &document_uri, document_revision.as_ref()).await;
        if allowed {
            assert!(
                knowledge::client::validate_read(
                    &conditional?,
                    &document_uri,
                    document_revision.as_ref()
                )?
                .unwrap()
                .not_modified()
            );
        } else {
            expect_mcp_error(
                conditional.unwrap_err(),
                rmcp::model::ErrorCode::INVALID_REQUEST,
            );
        }
        let result = client
            .read_resource(ReadResourceRequestParams::new(resource.as_str()))
            .await;
        if allowed {
            let result = result?;
            let text = result
                .contents
                .iter()
                .find_map(|value| match value {
                    ResourceContents::TextResourceContents { text, .. } => Some(text),
                    _ => None,
                })
                .expect("typed reading contents");
            let reading = serde_json::from_str::<Reading>(text)?;
            assert_eq!(
                reading,
                Reading {
                    id: ReadingId::new("sensor-a")?,
                    value: 7
                }
            );
            observed_reading = Some(reading);
            let missing = ObservatoryResource::Reading(ReadingId::new("missing")?).to_uri()?;
            expect_mcp_error(
                client
                    .read_resource(ReadResourceRequestParams::new(missing.as_str()))
                    .await
                    .unwrap_err(),
                // RMCP applies SEP-2164 for the negotiated 2026-07-28 protocol.
                rmcp::model::ErrorCode::INVALID_PARAMS,
            );
        } else {
            expect_mcp_error(result.unwrap_err(), rmcp::model::ErrorCode::INVALID_REQUEST);
            expect_mcp_error(
                client.list_resources(None).await.unwrap_err(),
                rmcp::model::ErrorCode::INVALID_REQUEST,
            );
            expect_mcp_error(
                client.list_resource_templates(None).await.unwrap_err(),
                rmcp::model::ErrorCode::INVALID_REQUEST,
            );
        }
        client.cancel().await?;
    }
    let label = veoveo_types::NamingLabel::new("observatory-reading")?;
    let bodies = [
        veoveo_mcp_conformance::OwnerSchemaEvidence::generated::<Reading>(label.clone()).observe(
            veoveo_mcp_conformance::SchemaObservation::from_serializable(
                &observed_reading.expect("successful authenticated reading"),
            )?,
        ),
    ];
    let evidence = veoveo_mcp_conformance::NamingEvidence {
        required_observations: vec![label],
        bodies: &bodies,
        ..Default::default()
    };
    let report = run_hosted_server_conformance_with_evidence(
        &profile,
        &ConformanceCredentials::bearer("fixture-read"),
        &Default::default(),
        &evidence,
    )
    .await?;
    println!("{}", serde_json::to_string(&report)?);
    assert!(report.passed(), "{:#?}", report.checks);
    let naming = report
        .checks
        .iter()
        .find(|c| c.requirement_id == "VV-MCP-NAMING-001")
        .unwrap();
    assert_eq!(
        naming.evidence.as_ref().unwrap()["outcome"],
        "review_required"
    );
    for id in ["K01", "K02", "K03", "K04", "K05", "K06"] {
        assert!(
            report.checks.iter().any(|check| check.requirement_id == id
                && check.status == veoveo_mcp_conformance::CheckStatus::Passed),
            "missing {id}"
        );
    }

    server.stop().await;
    assert!(
        tokio::net::TcpStream::connect(address).await.is_err(),
        "fixture listener outlived qualification"
    );
    Ok(())
}

struct OwnedReport(std::path::PathBuf);
impl Drop for OwnedReport {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn check_source_cli(endpoint: &str) -> anyhow::Result<()> {
    for (route, server, expected) in [
        ("direct", "observatory", true),
        ("gateway", "observatory", true),
        ("gateway", "absent", false),
    ] {
        let report = OwnedReport(
            std::env::temp_dir().join(format!("veoveo-source-cli-{}.json", uuid::Uuid::now_v7())),
        );
        let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_conformance"))
            .args([
                "knowledge-source",
                "--url",
                endpoint,
                "--server",
                server,
                "--owned-scheme",
                server,
                "--route",
                route,
                "--report",
            ])
            .arg(&report.0)
            .env("MCP_BEARER_TOKEN", "fixture-read")
            .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")
            .kill_on_drop(true)
            .output()
            .await?;
        assert_eq!(
            result.status.success(),
            expected,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let bytes = std::fs::read(&report.0)?;
        let decoded: veoveo_mcp_conformance::ConformanceReport = serde_json::from_slice(&bytes)?;
        assert_eq!(decoded.passed(), expected);
        assert_eq!(decoded.profile_id, format!("knowledge-{server}"));
        assert!(
            decoded
                .checks
                .iter()
                .all(|c| c.requirement_id.starts_with('K'))
        );
        assert!(!String::from_utf8_lossy(&bytes).contains("fixture-read"));
    }
    Ok(())
}

fn expect_mcp_error(error: rmcp::ServiceError, expected: rmcp::model::ErrorCode) {
    let rmcp::ServiceError::McpError(error) = error else {
        panic!("expected a protocol rejection, got {error:?}");
    };
    assert_eq!(error.code, expected, "{error:?}");
}

async fn knowledge_read(
    client: &rmcp::Peer<rmcp::RoleClient>,
    uri: &veoveo_types::ResourceUri,
    revision: Option<&veoveo_mcp_contract::docs::knowledge_extension::Revision>,
) -> Result<rmcp::model::ReadResourceResult, rmcp::ServiceError> {
    let (request, options) = veoveo_mcp_contract::docs::knowledge_extension::client::read_request(
        ReadResourceRequestParams::new(uri.as_str()),
        rmcp::model::ClientCapabilities::default(),
        revision,
        rmcp::service::PeerRequestOptions::with_timeout(Duration::from_secs(10)),
    );
    match client
        .send_request_with_option(request, options)
        .await?
        .await_response()
        .await?
    {
        rmcp::model::ServerResult::ReadResourceResult(result) => Ok(result),
        _ => Err(rmcp::ServiceError::UnexpectedResponse),
    }
}

/// Catalog faults wrap the existing source, retaining its domain/read authorization.
#[derive(Clone)]
struct CatalogFixture(std::sync::Arc<std::sync::atomic::AtomicU8>);

impl CatalogFixture {
    fn mode(&self) -> u8 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn metadata(
        &self,
        last: bool,
        surface: veoveo_gateway_contract::GatewayDiscoverySurface,
    ) -> Option<rmcp::model::MetaObject> {
        use veoveo_gateway_contract::{
            GatewayDiscoveryDegradation, GatewayDiscoveryFailure, GatewayDiscoveryFailureCode,
            GatewayDiscoverySurface,
        };
        use veoveo_mcp_contract::GatewayDiscoveryMetadata;
        let raw_mode = self.mode();
        if raw_mode >= 8 && surface != GatewayDiscoverySurface::Tools {
            return None;
        }
        let mode = if raw_mode >= 8 {
            raw_mode - 6
        } else {
            raw_mode
        };
        if mode == 0 || (!last && mode >= 5) {
            return None;
        }
        if mode == 3 || mode == 6 {
            let mut meta = rmcp::model::MetaObject::new();
            meta.insert(veoveo_gateway_contract::GATEWAY_DISCOVERY_DEGRADATION_META_KEY.into(), serde_json::json!({"failures":[{"server":"other","surface":"tools","code":"unsupported"}]}));
            return Some(meta);
        }
        GatewayDiscoveryDegradation::new([GatewayDiscoveryFailure {
            server: if mode == 2 || mode == 5 {
                "observatory"
            } else {
                "other"
            }
            .parse()
            .unwrap(),
            surface: if mode == 4 {
                GatewayDiscoverySurface::Resources
            } else {
                surface
            },
            code: GatewayDiscoveryFailureCode::UpstreamUnavailable,
        }])
        .into_meta()
    }
}

impl rmcp::ServerHandler for CatalogFixture {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        let mut info = ObservatoryMcp.get_info();
        if self.mode() != 0 {
            info.capabilities.tools = Some(Default::default());
        }
        info
    }
    async fn list_resource_templates(
        &self,
        request: Option<rmcp::model::PaginatedRequestParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListResourceTemplatesResult, rmcp::ErrorData> {
        let last = request
            .as_ref()
            .and_then(|request| request.cursor.as_ref())
            .is_some();
        let mut page = ObservatoryMcp
            .list_resource_templates(request, context)
            .await?;
        if self.mode() != 0 {
            if !last {
                page.resource_templates.clear();
                page.next_cursor = Some("templates-last".into());
            }
            page.meta = self.metadata(
                last,
                veoveo_gateway_contract::GatewayDiscoverySurface::ResourceTemplates,
            );
        }
        Ok(page)
    }
    async fn list_tools(
        &self,
        request: Option<rmcp::model::PaginatedRequestParams>,
        _: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        let last = request.and_then(|request| request.cursor).is_some();
        Ok(rmcp::model::ListToolsResult {
            tools: vec![],
            next_cursor: (!last).then(|| "tools-last".into()),
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(60_000),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: self.metadata(
                last,
                veoveo_gateway_contract::GatewayDiscoverySurface::Tools,
            ),
        })
    }
    async fn list_resources(
        &self,
        request: Option<rmcp::model::PaginatedRequestParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, rmcp::ErrorData> {
        ObservatoryMcp.list_resources(request, context).await
    }
    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, rmcp::ErrorData> {
        ObservatoryMcp.read_resource(request, context).await
    }
    async fn complete(
        &self,
        request: rmcp::model::CompleteRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CompleteResult, rmcp::ErrorData> {
        ObservatoryMcp.complete(request, context).await
    }
}

async fn qualify_selected_catalog(
    endpoint: &str,
    fault: &std::sync::atomic::AtomicU8,
    profile: &HostedServerConformanceProfile,
) -> anyhow::Result<()> {
    use veoveo_mcp_conformance::{
        KnowledgeRoute, KnowledgeSourceTarget, run_knowledge_source_conformance,
    };
    let target = |route| {
        KnowledgeSourceTarget::new(
            endpoint.parse().unwrap(),
            "observatory".parse().unwrap(),
            ["observatory".parse().unwrap()].into(),
            route,
        )
        .unwrap()
    };
    fault.store(1, std::sync::atomic::Ordering::SeqCst);
    let report = run_knowledge_source_conformance(
        &target(KnowledgeRoute::Gateway),
        &ConformanceCredentials::bearer("fixture-read"),
        &Default::default(),
    )
    .await?;
    assert!(report.passed(), "{:#?}", report.checks);
    let evidence = report
        .checks
        .iter()
        .find(|check| check.requirement_id == "K01")
        .unwrap()
        .evidence
        .as_ref()
        .unwrap();
    assert_eq!(evidence["collections"], 1);
    assert_eq!(
        evidence["selectedSourceCatalog"]["selectedServer"],
        "observatory"
    );
    assert_eq!(
        evidence["selectedSourceCatalog"]["wholeProfileComplete"],
        false
    );
    let failures = evidence["selectedSourceCatalog"]["unrelatedGatewayFailures"]
        .as_array()
        .unwrap();
    assert_eq!(failures.len(), 4);
    for (index, failure) in failures.iter().enumerate() {
        assert_eq!(failure["server"], "other");
        assert_eq!(failure["code"], "upstream_unavailable");
        assert_eq!(
            failure["surface"],
            if index < 2 {
                "resource_templates"
            } else {
                "tools"
            }
        );
    }
    assert!(
        run_knowledge_source_conformance(
            &target(KnowledgeRoute::Direct),
            &ConformanceCredentials::bearer("fixture-read"),
            &Default::default()
        )
        .await
        .is_err()
    );
    let mut full_profile = profile.clone();
    full_profile.surfaces.tools = SurfaceExpectation::Optional;
    let full = veoveo_mcp_conformance::run_hosted_server_conformance(
        &full_profile,
        &ConformanceCredentials::bearer("fixture-read"),
    )
    .await?;
    assert!(!full.passed());
    for requirement in ["VV-MCP-TOOLS-001", "VV-MCP-TEMPLATES-001"] {
        assert!(full.checks.iter().any(|check| {
            check.requirement_id == requirement
                && check.status == veoveo_mcp_conformance::CheckStatus::Failed
                && check
                    .summary
                    .contains("admitted gateway discovery failures")
        }));
    }
    let client = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(endpoint).auth_header("fixture-read"),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?;
    assert!(
        veoveo_mcp_conformance::catalog::templates(client.peer())
            .await
            .is_err()
    );
    assert!(
        veoveo_mcp_conformance::catalog::tools(client.peer())
            .await
            .is_err()
    );
    client.cancel().await?;
    for mode in [2, 3, 4, 5, 6, 8, 9, 10, 11, 12] {
        fault.store(mode, std::sync::atomic::Ordering::SeqCst);
        assert!(
            run_knowledge_source_conformance(
                &target(KnowledgeRoute::Gateway),
                &ConformanceCredentials::bearer("fixture-read"),
                &Default::default()
            )
            .await
            .is_err(),
            "catalog fault mode {mode} was accepted"
        );
    }
    fault.store(0, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}
