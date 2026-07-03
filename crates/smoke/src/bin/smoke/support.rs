use super::*;

pub(crate) const INTERNAL_SECRET: &str = "local-smoke-internal-token-secret-32-bytes-minimum";
pub(crate) const PUBLIC_BASE_URL: &str = "https://veoveo.bioma.ai";

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeGenerationRunOutput {
    pub(crate) artifacts: Vec<SmokeArtifactMetadata>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeArtifactMetadata {
    #[serde(default)]
    pub(crate) metadata: Value,
    #[serde(default)]
    pub(crate) compliance: SmokeCompliance,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SmokeCompliance {
    pub(crate) tenant_id: Option<String>,
    #[serde(default)]
    pub(crate) data_labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeUsageReport {
    pub(crate) task_id: String,
    pub(crate) usage_uri: String,
    pub(crate) records: Vec<SmokeUsageRecord>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeUsageRecord {
    pub(crate) task_id: String,
    pub(crate) kind: SmokeUsageKind,
    pub(crate) amount: Option<f64>,
    pub(crate) currency: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SmokeUsageKind {
    Estimate,
    Actual,
}

#[derive(Debug)]
pub(crate) struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    pub(crate) fn spawn(
        program: &Path,
        args: impl IntoIterator<Item = OsString>,
        envs: impl IntoIterator<Item = (&'static str, OsString)>,
        log: &Path,
    ) -> Result<Self> {
        let stdout = File::create(log)
            .with_context(|| format!("failed to create child log {}", log.display()))?;
        let stderr = stdout.try_clone()?;
        let mut command = Command::new(program);
        configure_binary_runtime(&mut command, program);
        let child = command
            .args(args)
            .envs(envs)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to spawn {}", program.display()))?;
        Ok(Self { child })
    }

    pub(crate) fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug)]
pub(crate) struct ContainerGuard {
    name: String,
}

impl ContainerGuard {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", self.name.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[derive(Clone, Default)]
pub(crate) struct SmokeMcpClient;

impl ClientHandler for SmokeMcpClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-smoke", env!("CARGO_PKG_VERSION")),
        )
    }
}

pub(crate) type SmokeMcpSession = RunningService<rmcp::RoleClient, SmokeMcpClient>;

pub(crate) fn spawn_fake_hosted_mcp(
    conformance: &Path,
    port: u16,
    server: &str,
    scheme: &str,
    ready_file: &Path,
    log: &Path,
) -> Result<ChildGuard> {
    ChildGuard::spawn(
        conformance,
        [
            "fake-hosted-mcp".into(),
            "--port".into(),
            port.to_string().into(),
            "--server".into(),
            server.into(),
            "--scheme".into(),
            scheme.into(),
            "--ready-file".into(),
            ready_file.as_os_str().to_os_string(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
        log,
    )
}

pub(crate) fn spawn_fake_media_provider(
    conformance: &Path,
    port: u16,
    ready_file: &Path,
    log: &Path,
    completion_delay_ms: Option<u64>,
) -> Result<ChildGuard> {
    let mut args = vec![
        "fake-media-provider".into(),
        "--port".into(),
        port.to_string().into(),
        "--ready-file".into(),
        ready_file.as_os_str().to_os_string(),
    ];
    if let Some(delay) = completion_delay_ms {
        args.push("--completion-delay-ms".into());
        args.push(delay.to_string().into());
    }
    ChildGuard::spawn(conformance, args, [], log)
}

pub(crate) fn spawn_media_s3_smoke(
    media: &Path,
    port: u16,
    public_base_url: &str,
    state_db: &Path,
    log: &Path,
) -> Result<ChildGuard> {
    ChildGuard::spawn(
        media,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--state-db".into(),
            state_db.as_os_str().to_os_string(),
            "--artifact-endpoint".into(),
            "http://127.0.0.1:9".into(),
            "--artifact-bucket".into(),
            "smoke-artifacts".into(),
            "--artifact-region".into(),
            "us-east-1".into(),
        ],
        [
            ("MEDIA_PROVIDER_API_KEY", "smoke".into()),
            ("AWS_ACCESS_KEY_ID", "smoke".into()),
            ("AWS_SECRET_ACCESS_KEY", "smoke".into()),
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
        ],
        log,
    )
}

pub(crate) fn spawn_media_memory_smoke(
    media: &Path,
    port: u16,
    public_base_url: &str,
    state_db: &Path,
    provider_base_url: &str,
    log: &Path,
) -> Result<ChildGuard> {
    ChildGuard::spawn(
        media,
        [
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            public_base_url.into(),
            "--state-db".into(),
            state_db.as_os_str().to_os_string(),
            "--artifact-store".into(),
            "memory".into(),
            "--provider-base-url".into(),
            provider_base_url.into(),
        ],
        [
            ("MEDIA_PROVIDER_WEBHOOK_SECRET", "".into()),
            ("MEDIA_PROVIDER_API_KEY", "smoke".into()),
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
        ],
        log,
    )
}

pub(crate) fn write_edge_caddyfile(path: &Path, gateway_port: u16, media_port: u16) -> Result<()> {
    let caddyfile = format!(
        r#"{{
    admin off
    auto_https off
}}

:8080 {{
    handle /mcp* {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /oauth* {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /.well-known/oauth-* {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /admin* {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /healthz {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /readyz {{
        reverse_proxy host.docker.internal:{gateway_port}
    }}
    handle /media/webhooks* {{
        reverse_proxy host.docker.internal:{media_port}
    }}
    handle /media/files* {{
        reverse_proxy host.docker.internal:{media_port}
    }}
    handle /media/artifacts* {{
        reverse_proxy host.docker.internal:{media_port}
    }}
    handle /media/healthz {{
        reverse_proxy host.docker.internal:{media_port}
    }}
    respond /media/mcp* 404
    respond 404
}}
"#
    );
    fs::write(path, caddyfile)?;
    Ok(())
}

pub(crate) fn gateway_serve_args(
    port: u16,
    control_plane: &Path,
    state_db: &Path,
) -> Vec<OsString> {
    vec![
        "serve".into(),
        "--port".into(),
        port.to_string().into(),
        "--public-base-url".into(),
        PUBLIC_BASE_URL.into(),
        "--control-plane".into(),
        control_plane.as_os_str().to_os_string(),
        "--state-db".into(),
        state_db.as_os_str().to_os_string(),
    ]
}

pub(crate) fn assert_executable(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("required binary does not exist: {}", path.display());
    }
    Ok(())
}

pub(crate) async fn wait_for_file_and_http(file: &Path, url: &str) -> Result<()> {
    for _ in 0..150 {
        if file.exists() && http_ok(url).await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {url} and {}", file.display());
}

pub(crate) async fn wait_for_file(file: &Path) -> Result<()> {
    for _ in 0..150 {
        if file.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {}", file.display());
}

pub(crate) async fn wait_for_file_contains(file: &Path, first: &str, second: &str) -> Result<()> {
    for _ in 0..80 {
        if let Ok(contents) = fs::read_to_string(file)
            && contents
                .lines()
                .any(|line| line.starts_with(first.trim_end()))
            && contents
                .lines()
                .any(|line| line.starts_with(second.trim_end()))
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let contents = fs::read_to_string(file).unwrap_or_default();
    bail!(
        "timed out waiting for `{first}` and `{second}` in {}\ncontents:\n{contents}",
        file.display()
    );
}

pub(crate) async fn wait_for_http(url: &str) -> Result<()> {
    for _ in 0..150 {
        if http_ok(url).await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {url}");
}

pub(crate) async fn wait_for_http_client(
    client: &reqwest::Client,
    url: &str,
    expected: StatusCode,
) -> Result<()> {
    for _ in 0..150 {
        if let Ok(response) = client.get(url).send().await
            && response.status() == expected
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {url} to return {expected}");
}

pub(crate) async fn http_ok(url: &str) -> Result<bool> {
    let response = reqwest::get(url).await;
    Ok(matches!(response, Ok(response) if response.status() == StatusCode::OK))
}

pub(crate) async fn assert_http_status(url: &str, expected: StatusCode) -> Result<()> {
    let status = reqwest::get(url).await?.status();
    if status == expected {
        Ok(())
    } else {
        bail!("expected {expected} from {url}, got {status}");
    }
}

pub(crate) async fn assert_http_post_status(
    url: &str,
    bearer_token: Option<&str>,
    expected: StatusCode,
) -> Result<()> {
    let client = reqwest::Client::new();
    let mut request = client.post(url);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    let status = request.send().await?.status();
    if status == expected {
        Ok(())
    } else {
        bail!("expected POST {url} to return {expected}, got {status}");
    }
}

pub(crate) async fn assert_ready_profiles(gateway_base: &str, expected: u64) -> Result<()> {
    let ready: Value = reqwest::get(format!("{gateway_base}/readyz"))
        .await?
        .error_for_status()?
        .json()
        .await?;
    if ready.get("profiles").and_then(Value::as_u64) == Some(expected) {
        Ok(())
    } else {
        bail!("gateway readyz did not report {expected} profile(s): {ready}");
    }
}

pub(crate) fn run_mcp(
    conformance: &Path,
    gateway_base: &str,
    token: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let mut all_args = vec!["--url".into(), format!("{gateway_base}/mcp/default").into()];
    all_args.extend(args);
    run_checked(conformance, all_args, [("MCP_BEARER_TOKEN", token.into())])
}

pub(crate) async fn connect_mcp_session(url: &str, bearer_token: &str) -> Result<SmokeMcpSession> {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url.to_string())
            .auth_header(bearer_token.to_string()),
    );
    Ok(SmokeMcpClient.serve(transport).await?)
}

pub(crate) async fn read_mcp_resource_json(session: &SmokeMcpSession, uri: &str) -> Result<Value> {
    let result = session
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    let Some(text) = result.contents.iter().find_map(|content| match content {
        ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
        _ => None,
    }) else {
        bail!("MCP resource `{uri}` did not return text content: {result:?}");
    };
    Ok(serde_json::from_str(text)?)
}

pub(crate) async fn assert_mcp_session_resource_denied(
    session: &SmokeMcpSession,
    uri: &str,
) -> Result<()> {
    if read_mcp_resource_json(session, uri).await.is_ok() {
        bail!("same MCP session unexpectedly read `{uri}` after policy update");
    }
    Ok(())
}

pub(crate) fn gateway_id_jag_token(
    conformance: &Path,
    gateway_base: &str,
    args: &[&str],
) -> Result<String> {
    gateway_id_jag_token_for_profile(conformance, gateway_base, "default", args)
}

pub(crate) fn gateway_id_jag_token_for_profile(
    conformance: &Path,
    gateway_base: &str,
    profile: &str,
    args: &[&str],
) -> Result<String> {
    let mut all_args = vec![
        "gateway-id-jag-token-exchange".into(),
        "--token-url".into(),
        format!("{gateway_base}/oauth/{profile}/token").into(),
        "--resource".into(),
        format!("{PUBLIC_BASE_URL}/mcp/{profile}").into(),
    ];
    all_args.extend(args.iter().map(|arg| OsString::from(*arg)));
    run_checked(conformance, all_args, [])
}

pub(crate) fn gateway_token(
    conformance: &Path,
    gateway_base: &str,
    args: &[&str],
) -> Result<String> {
    let mut all_args = vec![
        "gateway-token-exchange".into(),
        "--token-url".into(),
        format!("{gateway_base}/oauth/default/token").into(),
    ];
    all_args.extend(args.iter().map(|arg| OsString::from(*arg)));
    run_checked(conformance, all_args, [])
}

pub(crate) fn run_gateway_json(gateway: &Path, command: &str, state_db: &Path) -> Result<Value> {
    let output = run_checked(
        gateway,
        [
            command.into(),
            "--state-db".into(),
            state_db.as_os_str().to_os_string(),
        ],
        [],
    )?;
    Ok(serde_json::from_str(&output)?)
}

pub(crate) fn run_gateway_metadata_summary(
    gateway: &Path,
    state_db: &Path,
    metadata_key: &str,
) -> Result<Value> {
    let output = run_checked(
        gateway,
        [
            "audit-metadata-summary".into(),
            "--state-db".into(),
            state_db.as_os_str().to_os_string(),
            "--metadata-key".into(),
            metadata_key.into(),
        ],
        [],
    )?;
    Ok(serde_json::from_str(&output)?)
}

pub(crate) fn run_direct_mcp(
    conformance: &Path,
    url: &str,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let mut all_args = vec!["--url".into(), url.into()];
    all_args.extend(args);
    run_checked(conformance, all_args, envs)
}

pub(crate) fn run_checked(
    program: &Path,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let output = run_raw(program, args, envs)?;
    if !output.status.success() {
        bail!(
            "{} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            program.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub(crate) fn run_raw(
    program: &Path,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<Output> {
    let mut command = Command::new(program);
    configure_binary_runtime(&mut command, program);
    command
        .args(args)
        .env_remove("VEOVEO_INTERNAL_TOKEN_SECRET")
        .envs(envs)
        .output()
        .with_context(|| format!("failed to run {}", program.display()))
}

pub(crate) fn configure_binary_runtime(command: &mut Command, program: &Path) {
    let Some(bin_dir) = program.parent() else {
        return;
    };
    let deps_dir = bin_dir.join("deps");
    if !deps_dir.exists() {
        return;
    }
    prepend_path_env(command, "DYLD_LIBRARY_PATH", &deps_dir);
    prepend_path_env(command, "LD_LIBRARY_PATH", &deps_dir);
    prepend_path_env(command, "PATH", &deps_dir);
}

pub(crate) fn prepend_path_env(command: &mut Command, key: &str, path: &Path) {
    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = env::var_os(key) {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(paths) {
        command.env(key, joined);
    }
}

pub(crate) fn contains(haystack: &str, needle: &str) -> Result<()> {
    if haystack.contains(needle) {
        Ok(())
    } else {
        bail!("expected output to contain `{needle}`\noutput:\n{haystack}");
    }
}

pub(crate) fn https_client_with_ca(cert_path: &Path) -> Result<reqwest::Client> {
    let cert = reqwest::Certificate::from_pem(&fs::read(cert_path)?)?;
    Ok(reqwest::Client::builder()
        .add_root_certificate(cert)
        .redirect(Policy::none())
        .build()?)
}

pub(crate) fn redirect_location(
    response: reqwest::Response,
    expected: StatusCode,
) -> Result<String> {
    let status = response.status();
    if status != expected {
        bail!("expected redirect status {expected}, got {status}");
    }
    let location = response
        .headers()
        .get(LOCATION)
        .ok_or_else(|| anyhow!("redirect response had no Location header"))?
        .to_str()?
        .to_string();
    Ok(location)
}

pub(crate) async fn gateway_browser_authorization_code(
    http: &reqwest::Client,
    idp_client: &reqwest::Client,
    gateway_base: &str,
    idp_base: &str,
    code_challenge: &str,
    client_state: &str,
) -> Result<(String, String)> {
    let authorize_query = form_urlencoded(&[
        ("response_type", "code"),
        ("client_id", "veoveo-browser"),
        ("redirect_uri", "https://veoveo.bioma.ai/oauth/callback"),
        ("scope", "media:use"),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("state", client_state),
    ]);
    let authorize = http
        .get(format!(
            "{gateway_base}/oauth/default/authorize?{authorize_query}"
        ))
        .send()
        .await?;
    let authorize_location = redirect_location(authorize, StatusCode::FOUND)?;
    if !authorize_location.starts_with(&format!("{idp_base}/oauth2/authorize")) {
        bail!("unexpected authorize redirect: {authorize_location}");
    }

    let idp_authorize = idp_client.get(&authorize_location).send().await?;
    let idp_callback = redirect_location(idp_authorize, StatusCode::FOUND)?;
    if !idp_callback.starts_with("https://veoveo.bioma.ai/oauth/default/callback") {
        bail!("unexpected IdP callback redirect: {idp_callback}");
    }
    let callback_query = idp_callback
        .split_once('?')
        .map(|(_, query)| query.to_string())
        .ok_or_else(|| anyhow!("IdP callback had no query string: {idp_callback}"))?;
    let gateway_callback = http
        .get(format!(
            "{gateway_base}/oauth/default/callback?{callback_query}"
        ))
        .send()
        .await?;
    let client_redirect = redirect_location(gateway_callback, StatusCode::FOUND)?;
    if !client_redirect.starts_with("https://veoveo.bioma.ai/oauth/callback") {
        bail!("unexpected browser client redirect: {client_redirect}");
    }
    let gateway_code = url_query_value(&client_redirect, "code")?;
    Ok((gateway_code, callback_query))
}

pub(crate) fn url_query_value(url: &str, key: &str) -> Result<String> {
    let url = reqwest::Url::parse(url)?;
    url.query_pairs()
        .find_map(|(query_key, value)| (query_key == key).then(|| value.into_owned()))
        .ok_or_else(|| anyhow!("URL had no `{key}` query value: {url}"))
}

pub(crate) fn form_urlencoded(fields: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(fields.iter().copied());
    serializer.finish()
}

pub(crate) async fn post_json(
    client: &reqwest::Client,
    url: &str,
    bearer_token: Option<&str>,
    body: Value,
) -> Result<Value> {
    let mut request = client.post(url);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    if !body.is_null() {
        request = request.json(&body);
    }
    Ok(request.send().await?.error_for_status()?.json().await?)
}

pub(crate) async fn get_json(
    client: &reqwest::Client,
    url: &str,
    bearer_token: Option<&str>,
) -> Result<Value> {
    let mut request = client.get(url);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    Ok(request.send().await?.error_for_status()?.json().await?)
}

pub(crate) async fn put_json_file(
    client: &reqwest::Client,
    url: &str,
    bearer_token: Option<&str>,
    path: &Path,
) -> Result<Value> {
    let mut request = client
        .put(url)
        .header(CONTENT_TYPE, "application/json")
        .body(fs::read(path)?);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    Ok(request.send().await?.error_for_status()?.json().await?)
}

pub(crate) fn assert_control_plane_admin_result(
    value: &Value,
    expected_status: &str,
) -> Result<String> {
    assert_control_plane_admin_result_with_profiles(value, expected_status, 1)
}

pub(crate) fn assert_control_plane_admin_result_with_profiles(
    value: &Value,
    expected_status: &str,
    expected_profiles: u64,
) -> Result<String> {
    if value.get("status").and_then(Value::as_str) != Some(expected_status)
        || value.get("servers").and_then(Value::as_u64) != Some(1)
        || value.get("profiles").and_then(Value::as_u64) != Some(expected_profiles)
    {
        bail!("unexpected control-plane admin result: {value}");
    }
    let revision_id = value
        .get("revision_id")
        .and_then(Value::as_str)
        .filter(|revision_id| !revision_id.is_empty() && *revision_id != "null")
        .ok_or_else(|| anyhow!("control-plane admin result had no revision id: {value}"))?;
    Ok(revision_id.to_string())
}

pub(crate) fn assert_control_plane_status(value: &Value, expected_revision_id: &str) -> Result<()> {
    assert_control_plane_status_with_profiles(value, expected_revision_id, 1)
}

pub(crate) fn assert_control_plane_status_with_profiles(
    value: &Value,
    expected_revision_id: &str,
    expected_profiles: u64,
) -> Result<()> {
    if value.get("status").and_then(Value::as_str) != Some("ok")
        || value.get("servers").and_then(Value::as_u64) != Some(1)
        || value.get("profiles").and_then(Value::as_u64) != Some(expected_profiles)
        || value.get("revision_id").and_then(Value::as_str) != Some(expected_revision_id)
    {
        bail!("unexpected control-plane status: {value}");
    }
    Ok(())
}

pub(crate) fn jwt_id(token: &str) -> Result<String> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| anyhow!("JWT had no payload segment"))?;
    let payload: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;
    payload
        .get("jti")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("JWT payload had no jti: {payload}"))
}

pub(crate) fn assert_mcp_denied(
    conformance: &Path,
    mcp_url: &str,
    token: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<()> {
    let mut all_args = vec!["--url".into(), mcp_url.into()];
    all_args.extend(args);
    let output = run_raw(conformance, all_args, [("MCP_BEARER_TOKEN", token.into())])?;
    if output.status.success() {
        bail!(
            "MCP command was unexpectedly authorized\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(crate) fn write_cui_control_plane(input: &Path, output: &Path) -> Result<()> {
    let mut control_plane: Value = serde_json::from_str(&fs::read_to_string(input)?)?;
    let policies = control_plane
        .get_mut("policies")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no policies array"))?;
    let policy = policies
        .iter_mut()
        .find(|policy| policy.get("version").and_then(Value::as_str) == Some("2026-07-02"))
        .ok_or_else(|| anyhow!("control plane has no 2026-07-02 policy"))?;
    let rules = policy
        .get_mut("rules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("policy has no rules array"))?;
    let rule = rules
        .iter_mut()
        .find(|rule| rule.get("id").and_then(Value::as_str) == Some("allow_media_profile_use"))
        .ok_or_else(|| anyhow!("policy has no allow_media_profile_use rule"))?;
    rule["required_data_labels"] = serde_json::json!(["cui"]);
    rule["groups"] = serde_json::json!(["engineering"]);
    rule["roles"] = serde_json::json!(["operator"]);
    fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}

pub(crate) fn write_ops_profile_control_plane(input: &Path, output: &Path) -> Result<()> {
    let mut control_plane: Value = serde_json::from_str(&fs::read_to_string(input)?)?;

    let profiles = control_plane
        .get_mut("profiles")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no profiles array"))?;
    let default_profile = profiles
        .iter()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some("default"))
        .cloned()
        .ok_or_else(|| anyhow!("control plane has no default profile"))?;
    let mut ops_profile = default_profile;
    ops_profile["id"] = Value::String("ops".to_string());
    ops_profile["protected_resource"] = Value::String(format!("{PUBLIC_BASE_URL}/mcp/ops"));
    profiles.push(ops_profile);

    let policies = control_plane
        .get_mut("policies")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no policies array"))?;
    for rule in policies
        .iter_mut()
        .flat_map(|policy| policy.get_mut("rules").and_then(Value::as_array_mut))
        .flatten()
    {
        append_unique_string(rule, "profiles", "ops")?;
    }

    let oauth_clients = control_plane
        .get_mut("oauth_clients")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no oauth_clients array"))?;
    for client in oauth_clients {
        append_unique_string(client, "allowed_profiles", "ops")?;
    }

    fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}

pub(crate) fn append_unique_string(value: &mut Value, key: &str, item: &str) -> Result<()> {
    let values = value
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("JSON object has no `{key}` array"))?;
    if !values.iter().any(|value| value.as_str() == Some(item)) {
        values.push(Value::String(item.to_string()));
    }
    Ok(())
}

pub(crate) fn assert_schema_title(path: &Path, expected_title: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    if value.get("$schema").is_none() {
        bail!("schema {} has no `$schema` field", path.display());
    }
    if value.get("title").and_then(Value::as_str) != Some(expected_title) {
        bail!(
            "schema {} title was not `{expected_title}`: {value}",
            path.display()
        );
    }
    Ok(value)
}

pub(crate) fn assert_json_log(path: &Path, expected: &[(&str, &str)]) -> Result<()> {
    let contents = fs::read_to_string(path)?;
    for line in contents.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if expected
            .iter()
            .all(|(key, expected)| value.get(*key).and_then(Value::as_str) == Some(*expected))
        {
            return Ok(());
        }
    }
    bail!(
        "log {} did not contain JSON line with fields {:?}\ncontents:\n{}",
        path.display(),
        expected,
        contents
    );
}

pub(crate) fn smoke_tmpdir() -> Result<PathBuf> {
    let tmpdir = env::temp_dir().join(format!("veoveo-smoke-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmpdir)?;
    Ok(tmpdir)
}

pub(crate) fn assert_structured_field(output: &str, field: &str, expected: &str) -> Result<()> {
    let structured = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .ok_or_else(|| anyhow!("command output had no structured content:\n{output}"))?;
    let structured: Value = serde_json::from_str(structured)?;
    if structured.get(field).and_then(Value::as_str) == Some(expected) {
        Ok(())
    } else {
        bail!("structured field `{field}` did not equal `{expected}`: {structured}");
    }
}

pub(crate) fn task_id_from_output(output: &str) -> Result<String> {
    output
        .lines()
        .find_map(|line| {
            line.strip_prefix("task ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_string)
        })
        .ok_or_else(|| anyhow!("command output had no task id:\n{output}"))
}

pub(crate) fn structured_from_output<T: DeserializeOwned>(output: &str) -> Result<T> {
    let structured = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .ok_or_else(|| anyhow!("command output had no structured content:\n{output}"))?;
    Ok(serde_json::from_str(structured)?)
}

pub(crate) fn wait_for_actual_usage(
    conformance: &Path,
    mcp_url: &str,
    task_id: &str,
    bearer_token: Option<&str>,
) -> Result<SmokeUsageReport> {
    for _ in 0..90 {
        let envs = usage_envs(bearer_token);
        let output = run_raw(
            conformance,
            [
                "--url".into(),
                mcp_url.into(),
                "usage".into(),
                task_id.into(),
            ],
            envs,
        )?;
        if output.status.success() {
            let stdout = String::from_utf8(output.stdout)?;
            if let Ok(report) = serde_json::from_str::<SmokeUsageReport>(&stdout)
                && report
                    .records
                    .iter()
                    .any(|record| record.kind == SmokeUsageKind::Actual)
            {
                return Ok(report);
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    bail!("timed out waiting for actual usage for task `{task_id}`");
}

pub(crate) fn usage_envs(bearer_token: Option<&str>) -> Vec<(&'static str, OsString)> {
    match bearer_token {
        Some(token) => vec![("MCP_BEARER_TOKEN", token.into())],
        None => vec![("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    }
}

pub(crate) fn assert_usage_report(
    report: &SmokeUsageReport,
    scheme: &str,
    task_id: &str,
) -> Result<()> {
    if report.task_id != task_id {
        bail!(
            "usage report task id `{}` did not equal `{task_id}`",
            report.task_id
        );
    }
    let expected_uri = format!("{scheme}://usage/task/{task_id}");
    if report.usage_uri != expected_uri {
        bail!(
            "usage report URI `{}` did not equal `{expected_uri}`",
            report.usage_uri
        );
    }
    if report
        .records
        .iter()
        .any(|record| record.task_id != task_id)
    {
        bail!("usage report contained a record for a different task: {report:?}");
    }
    for expected_kind in [SmokeUsageKind::Estimate, SmokeUsageKind::Actual] {
        let found = report.records.iter().any(|record| {
            record.kind == expected_kind
                && record.amount == Some(0.01)
                && record.currency.as_deref() == Some("USD")
        });
        if !found {
            bail!("usage report missing {expected_kind:?} USD 0.01 record: {report:?}");
        }
    }
    Ok(())
}

pub(crate) fn assert_output_file(output_dir: &Path, extension: &str) -> Result<()> {
    if contains_nonempty_file_with_extension(output_dir, extension)? {
        Ok(())
    } else {
        bail!(
            "no non-empty .{extension} output file found under {}",
            output_dir.display()
        );
    }
}

pub(crate) fn contains_nonempty_file_with_extension(path: &Path, extension: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if contains_nonempty_file_with_extension(&path, extension)? {
                return Ok(true);
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some(extension)
            && entry.metadata()?.len() > 0
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn assert_audit_method(
    summary: &Value,
    method: &str,
    min_allow: u64,
    min_deny: u64,
) -> Result<()> {
    let rows = summary
        .as_array()
        .ok_or_else(|| anyhow!("audit summary is not an array"))?;
    let Some(row) = rows
        .iter()
        .find(|row| row.get("method").and_then(Value::as_str) == Some(method))
    else {
        bail!("audit summary missing method `{method}`: {summary}");
    };
    let allow = row
        .get("allow_events")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let deny = row
        .get("deny_events")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if allow >= min_allow && deny >= min_deny {
        Ok(())
    } else {
        bail!(
            "audit summary for `{method}` had allow={allow}, deny={deny}; expected allow>={min_allow}, deny>={min_deny}"
        );
    }
}

pub(crate) fn assert_json_u64_at_least(value: &Value, key: &str, minimum: u64) -> Result<()> {
    let actual = value.get(key).and_then(Value::as_u64).unwrap_or_default();
    if actual >= minimum {
        Ok(())
    } else {
        bail!("JSON field `{key}` was {actual}, expected at least {minimum}: {value}");
    }
}

pub(crate) fn assert_metadata_summary_at_least(
    summary: &Value,
    metadata_value: &str,
    minimum: u64,
) -> Result<()> {
    let rows = summary
        .as_array()
        .ok_or_else(|| anyhow!("metadata summary is not an array"))?;
    let events = rows
        .iter()
        .find(|row| row.get("metadata_value").and_then(Value::as_str) == Some(metadata_value))
        .and_then(|row| row.get("events").and_then(Value::as_u64))
        .unwrap_or_default();
    if events >= minimum {
        Ok(())
    } else {
        bail!(
            "metadata summary `{metadata_value}` had {events} event(s), expected at least {minimum}: {summary}"
        );
    }
}

pub(crate) fn assert_reason_summary_at_least(
    summary: &Value,
    reason: &str,
    minimum: u64,
) -> Result<()> {
    let rows = summary
        .as_array()
        .ok_or_else(|| anyhow!("reason summary is not an array"))?;
    let events = rows
        .iter()
        .find(|row| row.get("reason").and_then(Value::as_str) == Some(reason))
        .and_then(|row| row.get("events").and_then(Value::as_u64))
        .unwrap_or_default();
    if events >= minimum {
        Ok(())
    } else {
        bail!(
            "reason summary `{reason}` had {events} event(s), expected at least {minimum}: {summary}"
        );
    }
}

pub(crate) fn assert_no_audit_denies(summary: &Value) -> Result<()> {
    let rows = summary
        .as_array()
        .ok_or_else(|| anyhow!("audit summary is not an array"))?;
    for row in rows {
        let deny = row
            .get("deny_events")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if deny != 0 {
            bail!("audit summary had deny event: {row}");
        }
    }
    Ok(())
}

pub(crate) struct TmpDirGuard {
    path: PathBuf,
    remove_on_drop: bool,
}

impl TmpDirGuard {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            remove_on_drop: false,
        }
    }

    pub(crate) fn remove_on_drop(&mut self) {
        self.remove_on_drop = true;
    }
}

impl Drop for TmpDirGuard {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = std::fs::remove_dir_all(&self.path);
        } else {
            eprintln!(
                "smoke failed; leaving workspace for logs: {}",
                self.path.display()
            );
        }
    }
}
