use super::tokens::{CONFORMANCE_KEY_ID, conformance_encoding_key, conformance_jwks, unix_seconds};
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

#[derive(Debug, Deserialize)]
struct FakeOidcTokenRequest {
    grant_type: String,
    code: String,
    redirect_uri: String,
    client_id: String,
    client_secret: Option<String>,
    code_verifier: String,
}

#[derive(Debug, Serialize)]
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

/// Scripted OpenAI-compatible chat-completions endpoint for agent-kernel
/// smoke tests. The script is keyed off conversation shape, so it is
/// deterministic across retries:
///
/// - a message containing `Background task update` → final answer, stop;
/// - no assistant turns yet → call `media__run` (webhook-delayed task, so it
///   is guaranteed to outlive the episode and detach);
/// - otherwise → announce waiting, stop (the run detaches the pending task).
pub(super) async fn cmd_fake_openai_llm(port: u16, ready_file: Option<PathBuf>) -> Result<()> {
    let router = AxumRouter::new().route("/v1/chat/completions", axum_post(fake_llm_completion));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    axum::serve(listener, router).await?;
    Ok(())
}

async fn fake_llm_completion(AxumJson(request): AxumJson<Value>) -> AxumJson<Value> {
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let text_of = |message: &Value| match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let has_task_update = messages
        .iter()
        .any(|message| text_of(message).contains("Background task update"));
    let assistant_turns = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("assistant"))
        .count();

    let choice = if has_task_update {
        json!({
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "OBJECTIVE COMPLETE: the export artifact is recorded."
            },
            "finish_reason": "stop"
        })
    } else if assistant_turns == 0 {
        fake_llm_tool_call_choice(
            "media__run",
            json!({
                "model": "fake/image",
                "input": { "prompt": "agent kernel smoke" }
            }),
        )
    } else {
        json!({
            "index": 0,
            "message": { "role": "assistant", "content": "WAITING FOR BACKGROUND TASKS" },
            "finish_reason": "stop"
        })
    };

    AxumJson(json!({
        "id": "chatcmpl-fake",
        "object": "chat.completion",
        "created": 0,
        "model": request.get("model").cloned().unwrap_or_else(|| json!("fake")),
        "choices": [choice],
        "usage": { "prompt_tokens": 20, "completion_tokens": 10, "total_tokens": 30 }
    }))
}

fn fake_llm_tool_call_choice(tool_name: &str, arguments: Value) -> Value {
    json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": format!("call_{tool_name}"),
                "type": "function",
                "function": { "name": tool_name, "arguments": arguments.to_string() }
            }]
        },
        "finish_reason": "tool_calls"
    })
}

#[derive(Clone)]
struct FakeMediaProviderState {
    base_url: String,
    http: reqwest::Client,
    completion_delay: Duration,
}

#[derive(Debug, Deserialize)]
struct FakeBillingSearchRequest {
    #[serde(default)]
    prediction_uuids: Vec<String>,
}

pub(super) async fn cmd_fake_media_provider(
    port: u16,
    ready_file: Option<PathBuf>,
    completion_delay_ms: u64,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let base_url = format!("http://{}", listener.local_addr()?);
    let state = FakeMediaProviderState {
        base_url,
        http: reqwest::Client::new(),
        completion_delay: Duration::from_millis(completion_delay_ms),
    };
    let router = AxumRouter::new()
        .route("/api/v3/models", axum_get(fake_media_models))
        .route("/api/v3/billings/search", axum_post(fake_media_billing))
        .route("/api/v3/{*model_id}", axum_post(fake_media_submit))
        .route("/outputs/fake.png", axum_get(fake_media_output))
        .with_state(state);
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    axum::serve(listener, router).await?;
    Ok(())
}

fn fake_media_envelope(data: Value) -> AxumJson<Value> {
    AxumJson(json!({
        "code": 200,
        "message": "ok",
        "data": data,
    }))
}

async fn fake_media_models() -> AxumJson<Value> {
    fake_media_envelope(json!([
        {
            "model_id": "fake/image",
            "name": "Fake image",
            "type": "image-to-image",
            "description": "Deterministic local smoke-test model.",
            "base_price": 0.01,
            "formula": "fixed smoke price",
            "api_schema": {
                "api_schemas": [
                    {
                        "type": "model_run",
                        "request_schema": {
                            "type": "object",
                            "required": ["prompt"],
                            "properties": {
                                "prompt": { "type": "string" }
                            },
                            "additionalProperties": true
                        }
                    }
                ]
            }
        }
    ]))
}

async fn fake_media_submit(
    AxumState(state): AxumState<FakeMediaProviderState>,
    AxumPath(model_id): AxumPath<String>,
    AxumQuery(query): AxumQuery<BTreeMap<String, String>>,
    AxumJson(input): AxumJson<Value>,
) -> AxumJson<Value> {
    let prediction_id = format!("fake-{}", uuid::Uuid::new_v4());
    let output_url = format!("{}/outputs/fake.png", state.base_url);
    if let Some(webhook_url) = query.get("webhook").cloned() {
        let http = state.http.clone();
        let completion_delay = state.completion_delay;
        let terminal = json!({
            "id": prediction_id,
            "model": model_id,
            "outputs": [output_url],
            "status": "completed",
            "input": input,
            "executionTime": 0.2,
        });
        tokio::spawn(async move {
            tokio::time::sleep(completion_delay).await;
            if let Err(err) = http.post(webhook_url).json(&terminal).send().await {
                eprintln!("fake media provider webhook failed: {err}");
            }
        });
    }

    fake_media_envelope(json!({
        "id": prediction_id,
        "model": model_id,
        "outputs": [],
        "status": "processing",
    }))
}

async fn fake_media_billing(
    AxumJson(request): AxumJson<FakeBillingSearchRequest>,
) -> AxumJson<Value> {
    let prediction_id = request
        .prediction_uuids
        .first()
        .cloned()
        .unwrap_or_else(|| "fake-unknown".to_string());
    fake_media_envelope(json!({
        "items": [
            {
                "uuid": format!("billing-{prediction_id}"),
                "billing_type": "deduct",
                "price": 0.01,
                "created_at": Utc::now(),
                "updated_at": Utc::now(),
                "order": {
                    "uuid": format!("order-{prediction_id}"),
                    "state": "completed",
                    "status": "completed"
                },
                "prediction": {
                    "uuid": prediction_id,
                    "model_uuid": "fake/image",
                    "status": "completed"
                }
            }
        ]
    }))
}

async fn fake_media_output() -> impl AxumIntoResponse {
    let bytes = BASE64_STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=")
        .expect("valid embedded PNG");
    ([("content-type", "image/png")], bytes)
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
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .enable_completions()
            .build();
        let mut info = ServerInfo::default();
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
        .with_title(format!("{} run", self.server))
        .with_execution(ToolExecution::new().with_task_support(TaskSupport::Forbidden));
        Ok(ListToolsResult {
            tools: vec![tool],
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
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
        Ok(result)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        Ok(ListResourcesResult {
            resources: vec![
                Resource::new(self.scenarios_uri(), format!("{} scenarios", self.server))
                    .with_title(format!("{} scenario catalog", self.server))
                    .with_description("Deterministic fixture scenario catalog.")
                    .with_mime_type("application/json"),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, rmcp::ErrorData> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                ResourceTemplate::new(self.scenario_template(), "scenario")
                    .with_title(format!("{} scenario", self.server))
                    .with_description("Deterministic fixture scenario by id.")
                    .with_mime_type("application/json"),
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
        let text = if request.uri == self.scenarios_uri() {
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
            return Err(rmcp::ErrorData::resource_not_found(
                format!("unknown fixture resource `{}`", request.uri),
                None,
            ));
        }
        .map_err(|err| rmcp::ErrorData::internal_error(err.to_string(), None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, request.uri).with_mime_type("application/json"),
        ]))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, rmcp::ErrorData> {
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
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
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
    internal_token_secret: String,
    ready_file: Option<PathBuf>,
) -> Result<()> {
    let server_slug = ServerSlug::new(server.clone())?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        server_slug,
        InternalTokenSecret::new(internal_token_secret)?,
    );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let allowed_hosts = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let mcp_service = StreamableHttpService::new(
        {
            let server = server.clone();
            let scheme = scheme.clone();
            move || Ok(FakeHostedMcp::new(server.clone(), scheme.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts),
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
