use super::*;
#[derive(Clone)]
struct FakeOidcState {
    issuer: String,
    client_id: String,
    client_secret: String,
    codes: Arc<Mutex<BTreeMap<String, FakeOidcCode>>>,
}

#[derive(Debug, Clone)]
struct FakeOidcCode {
    nonce: String,
}

#[derive(Debug, Deserialize)]
struct FakeOidcAuthorizeRequest {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
    nonce: String,
}

#[derive(Deserialize)]
struct FakeOidcTokenRequest {
    grant_type: String,
    code: String,
    redirect_uri: String,
    client_id: String,
    client_secret: Option<String>,
    code_verifier: String,
}

#[derive(Serialize)]
struct FakeOidcTokenResponse {
    id_token: String,
    token_type: &'static str,
    expires_in: u64,
}

#[derive(Debug, Serialize)]
struct FakeOidcIdTokenClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    nonce: String,
    groups: Vec<String>,
    roles: Vec<String>,
    tenant: String,
    data_labels: Vec<String>,
    principal_assurances: Vec<String>,
    email: String,
}

pub(super) async fn cmd_gateway_fake_oidc_idp(
    port: u16,
    cert_pem: PathBuf,
    key_pem: PathBuf,
    ready_file: Option<PathBuf>,
    issuer: String,
    client_id: String,
    client_secret: String,
) -> Result<()> {
    let certified_key =
        generate_simple_self_signed(vec!["127.0.0.1".to_string(), "localhost".to_string()])?;
    if let Some(parent) = cert_pem.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = key_pem.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cert_pem, certified_key.cert.pem())?;
    std::fs::write(&key_pem, certified_key.signing_key.serialize_pem())?;

    let state = FakeOidcState {
        issuer,
        client_id,
        client_secret,
        codes: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let router = AxumRouter::new()
        .route("/.well-known/jwks.json", axum_get(fake_oidc_jwks))
        .route("/oauth2/authorize", axum_get(fake_oidc_authorize))
        .route("/oauth2/token", axum_post(fake_oidc_token))
        .with_state(state);
    let config = RustlsConfig::from_pem_file(&cert_pem, &key_pem).await?;
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    axum_server::bind_rustls(addr, config)
        .serve(router.into_make_service())
        .await?;
    Ok(())
}

#[derive(Clone)]
struct OtlpSinkState {
    hits_file: PathBuf,
}

pub(super) async fn cmd_otlp_http_sink(
    port: u16,
    ready_file: Option<PathBuf>,
    hits_file: PathBuf,
) -> Result<()> {
    if let Some(parent) = hits_file.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&hits_file, b"")?;
    let state = OtlpSinkState { hits_file };
    let router = AxumRouter::new()
        .route("/v1/{signal}", axum_post(otlp_sink_hit))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    axum::serve(listener, router).await?;
    Ok(())
}

#[derive(Clone)]
struct FakeHostedMcp {
    server: String,
    scheme: String,
}

#[derive(Clone)]
struct FakeHostedAuthState {
    verifier: GatewayInternalTokenVerifier,
}

impl FakeHostedMcp {
    fn new(server: impl Into<String>, scheme: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            scheme: scheme.into(),
        }
    }

    fn scenarios_uri(&self) -> String {
        format!("{}://scenarios", self.scheme)
    }

    fn is_chart_fixture(&self) -> bool {
        self.server == "charts"
    }

    fn chart_types_uri(&self) -> String {
        format!("{}://chart-types", self.scheme)
    }

    fn chart_view_uri(&self) -> &'static str {
        "ui://vendor/chart-view.html"
    }

    fn scenario_template(&self) -> String {
        format!("{}://scenario/{{scenario_id}}", self.scheme)
    }

    fn scenario_uri(&self, scenario_id: &str) -> String {
        format!("{}://scenario/{scenario_id}", self.scheme)
    }

    fn prompt_name(&self) -> String {
        format!("{}-plan", self.server)
    }

    fn scenario_ids(&self) -> [&'static str; 3] {
        ["orbital-docking", "supply-chain", "thermal-control"]
    }
}

impl ServerHandler for FakeHostedMcp {
    fn get_info(&self) -> ServerConfig {
        let mut caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .enable_completions()
            .build();
        if self.is_chart_fixture() {
            veoveo_mcp_apps_extension::extend_capabilities(&mut caps);
        }
        let mut info = ServerConfig::default();
        info.capabilities = caps;
        info.server_info = Implementation::new(self.server.clone(), env!("CARGO_PKG_VERSION"));
        info.instructions = Some(format!(
            "Generic hosted {} MCP fixture for gateway multi-server conformance.",
            self.server
        ));
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        if self.is_chart_fixture() {
            let input_schema: JsonObject = serde_json::from_value(json!({
                "type": "object",
                "properties": {
                    "chart_type": { "type": "string" },
                    "data": { "type": "array" }
                },
                "additionalProperties": true
            }))
            .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;
            return Ok(ListToolsResult {
                tools: vec![
                    Tool::new(
                        "render_chart",
                        "Render a deterministic chart fixture.",
                        input_schema.clone(),
                    )
                    .with_title("render chart"),
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        Tool::new(
                            "create_chart_view",
                            "Create a deterministic chart view fixture.",
                            input_schema,
                        )
                        .with_title("create chart view"),
                        self.chart_view_uri(),
                        &[
                            veoveo_mcp_apps_extension::UiVisibility::Model,
                            veoveo_mcp_apps_extension::UiVisibility::App,
                        ],
                    ),
                ],
                next_cursor: None,
                result_type: Some(rmcp::model::ResultType::COMPLETE),
                ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
                cache_scope: Some(rmcp::model::CacheScope::Private),
                meta: None,
            });
        }
        let input_schema: JsonObject = serde_json::from_value(json!({
            "type": "object",
            "required": ["scenario"],
            "properties": {
                "scenario": { "type": "string" }
            },
            "additionalProperties": false
        }))
        .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;
        let tool = Tool::new(
            "run",
            format!("Run a deterministic {} fixture scenario.", self.server),
            input_schema,
        )
        .with_title(format!("{} run", self.server));
        Ok(ListToolsResult {
            tools: vec![tool],
            next_cursor: None,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        if self.is_chart_fixture() {
            if request.name != "render_chart" && request.name != "create_chart_view" {
                return Err(rmcp::ErrorData::invalid_params(
                    format!("unknown tool `{}`", request.name),
                    None,
                ));
            }
            let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
                "{} fixture rendered chart view",
                self.server
            ))]);
            result.structured_content = Some(json!({
                "server": self.server,
                "chart_types_uri": self.chart_types_uri(),
                "view_resource_uri": self.chart_view_uri()
            }));
            return Ok(result.into());
        }
        if request.name != "run" {
            return Err(rmcp::ErrorData::invalid_params(
                format!("unknown tool `{}`", request.name),
                None,
            ));
        }
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let scenario = arguments
            .get("scenario")
            .and_then(Value::as_str)
            .ok_or_else(|| rmcp::ErrorData::invalid_params("missing scenario argument", None))?;
        let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
            "{} fixture accepted scenario {scenario}",
            self.server
        ))]);
        result.structured_content = Some(json!({
            "server": self.server,
            "scheme": self.scheme,
            "scenario": scenario,
            "scenario_uri": self.scenario_uri(scenario)
        }));
        Ok(result.into())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        if self.is_chart_fixture() {
            return Ok(ListResourcesResult {
                resources: vec![
                    Resource::new(self.chart_types_uri(), "chart types")
                        .with_title("chart types")
                        .with_description("Deterministic chart fixture type catalog.")
                        .with_mime_type("application/json"),
                    veoveo_mcp_apps_extension::app_resource(self.chart_view_uri(), "chart view")
                        .with_title("chart view")
                        .with_description("Deterministic chart fixture MCP App view."),
                ],
                next_cursor: None,
                result_type: Some(rmcp::model::ResultType::COMPLETE),
                ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
                cache_scope: Some(rmcp::model::CacheScope::Private),
                meta: None,
            });
        }
        Ok(ListResourcesResult {
            resources: vec![
                Resource::new(self.scenarios_uri(), format!("{} scenarios", self.server))
                    .with_title(format!("{} scenario catalog", self.server))
                    .with_description("Deterministic fixture scenario catalog.")
                    .with_mime_type("application/json"),
            ],
            next_cursor: None,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, rmcp::ErrorData> {
        if self.is_chart_fixture() {
            return Ok(ListResourceTemplatesResult {
                resource_templates: Vec::new(),
                next_cursor: None,
                result_type: Some(rmcp::model::ResultType::COMPLETE),
                ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
                cache_scope: Some(rmcp::model::CacheScope::Private),
                meta: None,
            });
        }
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                ResourceTemplate::new(self.scenario_template(), "scenario")
                    .with_title(format!("{} scenario", self.server))
                    .with_description("Deterministic fixture scenario by id.")
                    .with_mime_type("application/json"),
            ],
            next_cursor: None,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, rmcp::ErrorData> {
        let text = if self.is_chart_fixture() && request.uri == self.chart_types_uri() {
            serde_json::to_string(&json!({
                "server": self.server,
                "types": ["bar", "line"],
                "uri": self.chart_types_uri()
            }))
        } else if self.is_chart_fixture() && request.uri == self.chart_view_uri() {
            return Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(
                    &request.uri,
                    concat!(
                        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
                        "<title>chart view</title></head><body>",
                        "<p data-kind=\"chart_view\">charts fixture view</p>",
                        "<script>window.parent.postMessage(",
                        "{ jsonrpc: \"2.0\", id: 1, method: \"ui/initialize\", params: {} }, \"*\");",
                        "</script></body></html>"
                    ),
                ),
            ]).into());
        } else if request.uri == self.scenarios_uri() {
            serde_json::to_string(&json!({
                "server": self.server,
                "scheme": self.scheme,
                "scenarios": self.scenario_ids()
                    .into_iter()
                    .map(|scenario_id| json!({
                        "scenario_id": scenario_id,
                        "uri": self.scenario_uri(scenario_id)
                    }))
                    .collect::<Vec<_>>()
            }))
        } else if let Some(scenario_id) = request
            .uri
            .strip_prefix(&format!("{}://scenario/", self.scheme))
        {
            serde_json::to_string(&json!({
                "server": self.server,
                "scenario_id": scenario_id,
                "status": "available",
                "uri": self.scenario_uri(scenario_id)
            }))
        } else {
            return Err(rmcp::ErrorData::invalid_params(
                format!("unknown fixture resource `{}`", request.uri),
                None,
            ));
        }
        .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, request.uri).with_mime_type("application/json"),
        ])
        .into())
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, rmcp::ErrorData> {
        if self.is_chart_fixture() {
            return Ok(ListPromptsResult {
                prompts: vec![
                    Prompt::new(
                        "author_chart",
                        Some("Draft a deterministic chart fixture specification."),
                        Some(vec![
                            PromptArgument::new("chart_type")
                                .with_description("Chart type from the fixture catalog.")
                                .with_required(true),
                        ]),
                    )
                    .with_title("author chart"),
                ],
                next_cursor: None,
                result_type: Some(rmcp::model::ResultType::COMPLETE),
                ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
                cache_scope: Some(rmcp::model::CacheScope::Private),
                meta: None,
            });
        }
        Ok(ListPromptsResult {
            prompts: vec![
                Prompt::new(
                    self.prompt_name(),
                    Some(format!("Draft a {} fixture execution plan.", self.server)),
                    Some(vec![
                        PromptArgument::new("scenario")
                            .with_description("Scenario id from the fixture catalog.")
                            .with_required(true),
                    ]),
                )
                .with_title(format!("{} plan", self.server)),
            ],
            next_cursor: None,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResponse, rmcp::ErrorData> {
        async {
            if self.is_chart_fixture() {
                if request.name != "author_chart" {
                    return Err(rmcp::ErrorData::invalid_params(
                        format!("unknown prompt `{}`", request.name),
                        None,
                    ));
                }
                let chart_type = request
                    .arguments
                    .and_then(|args| args.get("chart_type").cloned())
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| "bar".to_string());
                return Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    Role::User,
                    format!(
                        "Author a {chart_type} chart using {}.",
                        self.chart_types_uri()
                    ),
                )])
                .with_description("chart fixture authoring prompt"));
            }
            if request.name != self.prompt_name() {
                return Err(rmcp::ErrorData::invalid_params(
                    format!("unknown prompt `{}`", request.name),
                    None,
                ));
            }
            let scenario = request
                .arguments
                .and_then(|args| args.get("scenario").cloned())
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unspecified".to_string());
            Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                Role::User,
                format!(
                    "Prepare a {} fixture plan for scenario `{scenario}`. Read {} first.",
                    self.server,
                    self.scenario_uri(&scenario)
                ),
            )])
            .with_description(format!("{} fixture plan", self.server)))
        }
        .await
        .map(Into::into)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, rmcp::ErrorData> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        if reference.uri != self.scenario_template() || request.argument.name != "scenario_id" {
            return Ok(CompleteResult::default());
        }
        let prefix = request.argument.value.to_lowercase();
        let values = self
            .scenario_ids()
            .into_iter()
            .filter(|scenario_id| scenario_id.starts_with(&prefix))
            .map(str::to_string)
            .collect::<Vec<_>>();
        let completion =
            CompletionInfo::with_pagination(values.clone(), Some(values.len() as u32), false)
                .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;
        Ok(CompleteResult::new(completion))
    }
}

pub(super) async fn cmd_fake_hosted_mcp(
    port: u16,
    server: String,
    scheme: String,
    internal_trust_jwks: String,
    ready_file: Option<PathBuf>,
) -> Result<()> {
    let server_slug = ServerSlug::parse(server.clone())?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        server_slug,
        GatewayInternalTrustBundle::from_json(&internal_trust_jwks)?,
    );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let allowed_hosts = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let mcp_service = StreamableHttpService::new(
        {
            let server = server.clone();
            let scheme = scheme.clone();
            move || Ok(FakeHostedMcp::new(server.clone(), scheme.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts),
    );
    let mcp_router = AxumRouter::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(axum_middleware::from_fn_with_state(
            FakeHostedAuthState { verifier },
            authenticate_fake_hosted_mcp,
        ));
    let router = AxumRouter::new().nest(
        &format!("/{server}"),
        AxumRouter::new()
            .route("/healthz", axum_get(|| async { "ok" }))
            .nest("/mcp", mcp_router),
    );
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    axum::serve(listener, router).await?;
    Ok(())
}

async fn authenticate_fake_hosted_mcp(
    AxumState(state): AxumState<FakeHostedAuthState>,
    mut request: AxumRequest,
    next: AxumNext,
) -> axum::response::Response {
    match verify_fake_hosted_internal_authorization(&state.verifier, request.headers()) {
        Ok(identity) => {
            request
                .extensions_mut()
                .insert::<GatewayInternalIdentity>(identity);
            next.run(request).await
        }
        Err(message) => (AxumStatusCode::UNAUTHORIZED, message).into_response(),
    }
}

fn verify_fake_hosted_internal_authorization(
    verifier: &GatewayInternalTokenVerifier,
    headers: &AxumHeaderMap,
) -> Result<GatewayInternalIdentity, String> {
    let header = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "missing internal authorization".to_string())?;
    let Some((scheme, token)) = header.split_once(' ') else {
        return Err("missing bearer token".to_string());
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err("authorization scheme must be Bearer".to_string());
    }
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err("bearer token contains invalid whitespace".to_string());
    }
    verifier.verify(token).map_err(|err| err.to_string())
}

async fn otlp_sink_hit(
    AxumState(state): AxumState<OtlpSinkState>,
    AxumPath(signal): AxumPath<String>,
    body: AxumBytes,
) -> impl AxumIntoResponse {
    match signal.as_str() {
        "logs" | "traces" | "metrics" => {
            use std::io::Write as _;

            let line = format!("{signal} {}\n", body.len());
            let result = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&state.hits_file)
                .and_then(|mut file| file.write_all(line.as_bytes()));
            match result {
                Ok(()) => AxumStatusCode::OK,
                Err(_) => AxumStatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        _ => AxumStatusCode::NOT_FOUND,
    }
}

async fn fake_oidc_jwks() -> impl AxumIntoResponse {
    match conformance_jwks() {
        Ok(jwks) => AxumJson(jwks).into_response(),
        Err(err) => (
            AxumStatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to build JWKS: {err}"),
        )
            .into_response(),
    }
}

async fn fake_oidc_authorize(
    AxumState(state): AxumState<FakeOidcState>,
    AxumQuery(request): AxumQuery<FakeOidcAuthorizeRequest>,
) -> impl AxumIntoResponse {
    if request.response_type != "code"
        || request.client_id != state.client_id
        || request.code_challenge_method != "S256"
        || request.code_challenge.is_empty()
        || !request
            .scope
            .split_whitespace()
            .any(|scope| scope == "openid")
    {
        return (AxumStatusCode::BAD_REQUEST, "invalid authorization request").into_response();
    }
    let code = format!("idp-code-{}", uuid::Uuid::new_v4().simple());
    match state.codes.lock() {
        Ok(mut codes) => {
            codes.insert(
                code.clone(),
                FakeOidcCode {
                    nonce: request.nonce,
                },
            );
        }
        Err(_) => {
            return (
                AxumStatusCode::INTERNAL_SERVER_ERROR,
                "code store unavailable",
            )
                .into_response();
        }
    }
    let mut redirect = match Url::parse(&request.redirect_uri) {
        Ok(url) => url,
        Err(_) => return (AxumStatusCode::BAD_REQUEST, "invalid redirect_uri").into_response(),
    };
    redirect
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &request.state);
    (
        AxumStatusCode::FOUND,
        [(axum::http::header::LOCATION, redirect.to_string())],
    )
        .into_response()
}

async fn fake_oidc_token(
    AxumState(state): AxumState<FakeOidcState>,
    AxumForm(request): AxumForm<FakeOidcTokenRequest>,
) -> impl AxumIntoResponse {
    if request.grant_type != "authorization_code"
        || request.client_id != state.client_id
        || request.client_secret.as_deref() != Some(state.client_secret.as_str())
        || request.redirect_uri.is_empty()
        || request.code_verifier.is_empty()
    {
        return (AxumStatusCode::UNAUTHORIZED, "invalid token request").into_response();
    }
    let code = match state.codes.lock() {
        Ok(mut codes) => codes.remove(&request.code),
        Err(_) => {
            return (
                AxumStatusCode::INTERNAL_SERVER_ERROR,
                "code store unavailable",
            )
                .into_response();
        }
    };
    let Some(code) = code else {
        return (AxumStatusCode::BAD_REQUEST, "invalid authorization code").into_response();
    };
    match fake_oidc_id_token(&state, &code) {
        Ok(id_token) => AxumJson(FakeOidcTokenResponse {
            id_token,
            token_type: "Bearer",
            expires_in: 300,
        })
        .into_response(),
        Err(err) => (
            AxumStatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to sign ID token: {err}"),
        )
            .into_response(),
    }
}

fn fake_oidc_id_token(state: &FakeOidcState, code: &FakeOidcCode) -> Result<String> {
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::minutes(5))
        .ok_or_else(|| anyhow!("ID token expiration overflow"))?;
    let claims = FakeOidcIdTokenClaims {
        iss: state.issuer.clone(),
        sub: "00u-browser-smoke".to_string(),
        aud: state.client_id.clone(),
        exp: unix_seconds(expires_at.timestamp())?,
        nbf: unix_seconds(now.timestamp())?,
        iat: unix_seconds(now.timestamp())?,
        nonce: code.nonce.clone(),
        groups: vec!["engineering".to_string()],
        roles: vec!["operator".to_string()],
        tenant: "tenant-a".to_string(),
        data_labels: vec!["cui".to_string()],
        principal_assurances: vec!["us_person".to_string()],
        email: "browser-smoke@example.com".to_string(),
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(CONFORMANCE_KEY_ID.to_string());
    Ok(encode(&header, &claims, &conformance_encoding_key()?)?)
}
