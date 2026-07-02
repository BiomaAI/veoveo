use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::{Parser, Subcommand};
use reqwest::{
    StatusCode,
    header::{CONTENT_TYPE, LOCATION},
    redirect::Policy,
};
use rmcp::{
    ClientHandler, ServiceExt,
    model::{
        ClientCapabilities, ClientInfo, Implementation, ReadResourceRequestParams, ResourceContents,
    },
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

const INTERNAL_SECRET: &str = "local-smoke-internal-token-secret-32-bytes-minimum";
const PUBLIC_BASE_URL: &str = "https://veoveo.bioma.ai";

#[derive(Debug, Deserialize)]
struct SmokeGenerationRunOutput {
    artifacts: Vec<SmokeArtifactMetadata>,
}

#[derive(Debug, Deserialize)]
struct SmokeArtifactMetadata {
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    compliance: SmokeCompliance,
}

#[derive(Debug, Default, Deserialize)]
struct SmokeCompliance {
    tenant_id: Option<String>,
    #[serde(default)]
    data_labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SmokeUsageReport {
    task_id: String,
    usage_uri: String,
    records: Vec<SmokeUsageRecord>,
}

#[derive(Debug, Deserialize)]
struct SmokeUsageRecord {
    task_id: String,
    kind: SmokeUsageKind,
    amount: Option<f64>,
    currency: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SmokeUsageKind {
    Estimate,
    Actual,
}

#[derive(Parser, Debug)]
#[command(name = "smoke", about = "Veoveo smoke-test harness")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the full production gateway smoke suite.
    GatewaySuite {
        /// Local gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.local.json")]
        control_plane: PathBuf,
        /// Gateway control-plane JSON used by smoke scenarios.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        smoke_control_plane: PathBuf,
    },
    /// Smoke-test Compose edge routing and published-port shape.
    ComposeConfig,
    /// Smoke-test contract schema export for external implementations.
    ContractSchemas {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
    },
    /// Smoke-test OTLP HTTP log and trace export from the gateway.
    Otel {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    /// Smoke-test the media MCP HTTP boundary and internal assertion requirement.
    MediaMcpAuth {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built media MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        media_bin: PathBuf,
    },
    /// Smoke-test direct hosted media task behavior without gateway projection.
    MediaTaskRun {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built media MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        media_bin: PathBuf,
    },
    /// Smoke-test the gateway HTTP boundary, auth discovery, and browser OAuth flow.
    GatewayHttp {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    /// Smoke-test authenticated gateway-to-media forwarding and policy/admin flows.
    GatewayAuthenticated {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built media MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        media_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    /// Run one gateway profile against two hosted MCP upstreams.
    GatewayTwoServers {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    /// Smoke-test a full gateway task run with webhook completion and usage.
    GatewayTaskRun {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built media MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        media_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
}

#[derive(Debug)]
struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    fn spawn(
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

    fn stop(&mut self) {
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
struct ContainerGuard {
    name: String,
}

impl ContainerGuard {
    fn new(name: impl Into<String>) -> Self {
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
struct SmokeMcpClient;

impl ClientHandler for SmokeMcpClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-smoke", env!("CARGO_PKG_VERSION")),
        )
    }
}

type SmokeMcpSession = RunningService<rmcp::RoleClient, SmokeMcpClient>;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    match args.cmd {
        Cmd::GatewaySuite {
            control_plane,
            smoke_control_plane,
        } => gateway_suite(&control_plane, &smoke_control_plane).await,
        Cmd::ComposeConfig => compose_config().await,
        Cmd::ContractSchemas { conformance_bin } => contract_schemas(&conformance_bin),
        Cmd::Otel {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => otel(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::MediaMcpAuth {
            conformance_bin,
            media_bin,
        } => media_mcp_auth(&conformance_bin, &media_bin).await,
        Cmd::MediaTaskRun {
            conformance_bin,
            media_bin,
        } => media_task_run(&conformance_bin, &media_bin).await,
        Cmd::GatewayHttp {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_http(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::GatewayAuthenticated {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
        } => {
            gateway_authenticated(&conformance_bin, &media_bin, &gateway_bin, &control_plane).await
        }
        Cmd::GatewayTwoServers {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_two_servers(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::GatewayTaskRun {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
        } => gateway_task_run(&conformance_bin, &media_bin, &gateway_bin, &control_plane).await,
    }
}

async fn gateway_suite(control_plane: &Path, smoke_control_plane: &Path) -> Result<()> {
    let conformance = Path::new("target/debug/conformance");
    let gateway = Path::new("target/debug/gateway");
    let media = Path::new("target/debug/server");

    suite_step("workspace contract and gateway tests");
    run_checked(
        Path::new("cargo"),
        [
            "test".into(),
            "-p".into(),
            "veoveo-mcp-contract".into(),
            "-p".into(),
            "veoveo-mcp-gateway".into(),
        ],
        [],
    )?;

    suite_step("smoke binary dependencies");
    run_checked(
        Path::new("cargo"),
        [
            "build".into(),
            "-p".into(),
            "veoveo-mcp-contract".into(),
            "--bin".into(),
            "conformance".into(),
            "-p".into(),
            "veoveo-mcp-gateway".into(),
            "--bin".into(),
            "gateway".into(),
            "-p".into(),
            "veoveo-media-mcp".into(),
            "--bin".into(),
            "server".into(),
        ],
        [],
    )?;

    suite_step("contract schema export");
    contract_schemas(conformance)?;

    suite_step("gateway control-plane validation");
    run_checked(
        gateway,
        [
            "validate".into(),
            "--control-plane".into(),
            control_plane.as_os_str().to_os_string(),
        ],
        [],
    )?;
    run_checked(
        gateway,
        [
            "validate".into(),
            "--control-plane".into(),
            smoke_control_plane.as_os_str().to_os_string(),
        ],
        [],
    )?;

    suite_step("self-hosted deployment validation");
    run_checked(
        conformance,
        [
            "deployment-validate".into(),
            "--file".into(),
            "configs/deployments.json".into(),
        ],
        [],
    )?;

    suite_step("compose edge configuration");
    compose_config().await?;

    suite_step("gateway HTTP and OAuth boundary");
    gateway_http(conformance, gateway, smoke_control_plane).await?;

    suite_step("gateway OpenTelemetry export");
    otel(conformance, gateway, smoke_control_plane).await?;

    suite_step("media MCP auth boundary");
    media_mcp_auth(conformance, media).await?;

    suite_step("direct media task run");
    media_task_run(conformance, media).await?;

    suite_step("authenticated gateway forwarding and policy");
    gateway_authenticated(conformance, media, gateway, smoke_control_plane).await?;

    suite_step("gateway with two hosted servers");
    gateway_two_servers(conformance, gateway, smoke_control_plane).await?;

    suite_step("gateway task run with artifacts and usage");
    gateway_task_run(conformance, media, gateway, smoke_control_plane).await?;

    println!("gateway smoke suite ok");
    Ok(())
}

fn suite_step(name: &str) {
    println!("==> {name}");
}

async fn compose_config() -> Result<()> {
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let compose_output = run_checked(
        Path::new("docker"),
        [
            "compose".into(),
            "-f".into(),
            "compose.yaml".into(),
            "-f".into(),
            "compose.tunnel.yaml".into(),
            "--profile".into(),
            "dev".into(),
            "--profile".into(),
            "tunnel".into(),
            "config".into(),
        ],
        [
            ("MEDIA_PROVIDER_API_KEY", "dummy".into()),
            (
                "MEDIA_PROVIDER_WEBHOOK_SECRET",
                "whsec_0Wn4SW+lD1zrRtFhb1r4fGHt6XZLSkX5y2EK+lSbA+E=".into(),
            ),
            (
                "VEOVEO_INTERNAL_TOKEN_SECRET",
                "local-development-secret-at-least-32-bytes".into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                "dummy".into(),
            ),
            ("PUBLIC_BASE_URL", PUBLIC_BASE_URL.into()),
            ("CLOUDFLARED_TUNNEL_TOKEN", "dummy".into()),
        ],
    )?;
    let host_ip_count = compose_output.matches("host_ip: 127.0.0.1").count();
    if host_ip_count < 7 {
        bail!("compose config had {host_ip_count} loopback port bindings; expected at least 7");
    }
    for expected in [
        "image: caddy:2.11.2",
        "target: /etc/caddy/Caddyfile",
        "target: 8080",
        "published: \"8780\"",
        "edge:",
    ] {
        contains(&compose_output, expected)?;
    }

    let gateway_dockerfile = fs::read_to_string("crates/mcp-gateway/Dockerfile")?;
    contains(&gateway_dockerfile, "find /app/target -name 'libduckdb.so'")?;
    contains(
        &gateway_dockerfile,
        "COPY --from=builder /out/lib/libduckdb.so /usr/local/lib/libduckdb.so",
    )?;

    let caddyfile = env::current_dir()?.join("configs/Caddyfile");
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--rm".into(),
            "-v".into(),
            format!("{}:/etc/caddy/Caddyfile:ro", caddyfile.display()).into(),
            "caddy:2.11.2".into(),
            "caddy".into(),
            "validate".into(),
            "--config".into(),
            "/etc/caddy/Caddyfile".into(),
            "--adapter".into(),
            "caddyfile".into(),
        ],
        [],
    )?;

    cleanup.remove_on_drop();
    println!("compose config smoke ok");
    Ok(())
}

fn contract_schemas(conformance: &Path) -> Result<()> {
    assert_executable(conformance)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());
    let schemas = tmpdir.join("schemas");

    run_checked(
        conformance,
        [
            "contract-schemas".into(),
            "--output-dir".into(),
            schemas.as_os_str().to_os_string(),
        ],
        [],
    )?;

    assert_schema_title(
        &schemas.join("gateway-control-plane.schema.json"),
        "GatewayControlPlane",
    )?;
    let artifact = assert_schema_title(
        &schemas.join("artifact-metadata.schema.json"),
        "ArtifactMetadata",
    )?;
    if !artifact
        .get("properties")
        .and_then(|properties| properties.get("compliance"))
        .is_some_and(Value::is_object)
    {
        bail!("artifact metadata schema has no object compliance property");
    }
    let usage = assert_schema_title(&schemas.join("usage-report.schema.json"), "UsageReport")?;
    if !usage
        .get("properties")
        .and_then(|properties| properties.get("records"))
        .is_some_and(Value::is_object)
    {
        bail!("usage report schema has no object records property");
    }

    cleanup.remove_on_drop();
    println!("contract schemas smoke ok");
    Ok(())
}

async fn otel(conformance: &Path, gateway: &Path, control_plane: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let gateway_port = 18804u16;
    let otlp_port = 18805u16;
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let otlp_base = format!("http://127.0.0.1:{otlp_port}");
    let gateway_log = tmpdir.join("gateway.log");
    let otlp_log = tmpdir.join("otlp.log");
    let otlp_ready = tmpdir.join("otlp.ready");
    let otlp_hits = tmpdir.join("otlp.hits");
    let state_db = tmpdir.join("gateway-state.duckdb");

    let mut otlp = ChildGuard::spawn(
        conformance,
        [
            "otlp-http-sink".into(),
            "--port".into(),
            otlp_port.to_string().into(),
            "--ready-file".into(),
            otlp_ready.as_os_str().to_os_string(),
            "--hits-file".into(),
            otlp_hits.as_os_str().to_os_string(),
        ],
        [],
        &otlp_log,
    )?;
    wait_for_file(&otlp_ready).await?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        [
            "serve".into(),
            "--port".into(),
            gateway_port.to_string().into(),
            "--public-base-url".into(),
            PUBLIC_BASE_URL.into(),
            "--control-plane".into(),
            control_plane.as_os_str().to_os_string(),
            "--state-db".into(),
            state_db.as_os_str().to_os_string(),
        ],
        [
            ("OTEL_EXPORTER_OTLP_ENDPOINT", otlp_base.into()),
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    let ready: Value = reqwest::get(format!("{gateway_base}/readyz"))
        .await?
        .error_for_status()?
        .json()
        .await?;
    if ready.get("profiles").and_then(Value::as_u64) != Some(1) {
        bail!("gateway readyz did not report one profile: {ready}");
    }

    wait_for_file_contains(&otlp_hits, "logs ", "traces ").await?;

    gateway_child.stop();
    otlp.stop();
    cleanup.remove_on_drop();
    println!("otel smoke ok");
    Ok(())
}

async fn gateway_http(conformance: &Path, gateway: &Path, base_control_plane: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let port = 18799u16;
    let idp_port = 18803u16;
    let base = format!("http://127.0.0.1:{port}");
    let idp_base = format!("https://127.0.0.1:{idp_port}");
    let gateway_log = tmpdir.join("gateway.log");
    let idp_log = tmpdir.join("idp.log");
    let state_db = tmpdir.join("state.duckdb");
    let control_plane = tmpdir.join("gateway.smoke.json");
    let idp_cert = tmpdir.join("idp-cert.pem");
    let idp_key = tmpdir.join("idp-key.pem");
    let idp_ready = tmpdir.join("idp.ready");
    let oidc_secret = "local-smoke-oidc-client-secret";

    let mut idp = ChildGuard::spawn(
        conformance,
        [
            "gateway-fake-oidc-idp".into(),
            "--port".into(),
            idp_port.to_string().into(),
            "--cert-pem".into(),
            idp_cert.as_os_str().to_os_string(),
            "--key-pem".into(),
            idp_key.as_os_str().to_os_string(),
            "--ready-file".into(),
            idp_ready.as_os_str().to_os_string(),
        ],
        [("VEOVEO_IDP_OIDC_CLIENT_SECRET", oidc_secret.into())],
        &idp_log,
    )?;
    wait_for_file(&idp_ready).await?;
    let idp_client = https_client_with_ca(&idp_cert)?;
    wait_for_http_client(
        &idp_client,
        &format!("{idp_base}/.well-known/jwks.json"),
        StatusCode::OK,
    )
    .await?;
    let idp_jwks: Value = idp_client
        .get(format!("{idp_base}/.well-known/jwks.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if !idp_jwks
        .get("keys")
        .and_then(Value::as_array)
        .is_some_and(|keys| {
            keys.iter()
                .any(|key| key.get("kid").and_then(Value::as_str) == Some("test-key"))
        })
    {
        bail!("fake IdP JWKS did not expose test-key: {idp_jwks}");
    }

    run_checked(
        conformance,
        [
            "gateway-smoke-control-plane".into(),
            "--base".into(),
            base_control_plane.as_os_str().to_os_string(),
            "--output".into(),
            control_plane.as_os_str().to_os_string(),
            "--idp-base-url".into(),
            idp_base.clone().into(),
            "--trusted-ca-path".into(),
            idp_cert.as_os_str().to_os_string(),
        ],
        [],
    )?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(port, &control_plane, &state_db),
        [
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
            ("VEOVEO_IDP_OIDC_CLIENT_SECRET", oidc_secret.into()),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{base}/healthz")).await?;
    assert_ready_profiles(&base, 1).await?;
    assert_json_log(
        &gateway_log,
        &[("message", "listening"), ("service", "veoveo-mcp-gateway")],
    )?;
    assert_json_log(
        &gateway_log,
        &[("message", "gateway retention gc completed")],
    )?;

    run_checked(
        conformance,
        [
            "--url".into(),
            format!("{base}/mcp/default").into(),
            "auth-discovery".into(),
            "--metadata-url".into(),
            format!("{base}/.well-known/oauth-protected-resource/mcp/default").into(),
            "--authorization-server-metadata-url".into(),
            format!("{base}/.well-known/oauth-authorization-server/oauth/default").into(),
            "--authorization-server-jwks-url".into(),
            format!("{base}/oauth/default/jwks.json").into(),
            "--required-scope".into(),
            "media:use".into(),
            "--required-extension".into(),
            "io.modelcontextprotocol/enterprise-managed-authorization".into(),
            "--required-extension".into(),
            "io.modelcontextprotocol/oauth-client-credentials".into(),
            "--required-jwks-key-id".into(),
            "test-key".into(),
            "--required-grant-type".into(),
            "authorization_code".into(),
            "--required-grant-type".into(),
            "client_credentials".into(),
            "--required-grant-type".into(),
            "urn:ietf:params:oauth:grant-type:jwt-bearer".into(),
            "--required-grant-profile".into(),
            "urn:ietf:params:oauth:grant-profile:id-jag".into(),
            "--required-token-auth-method".into(),
            "none".into(),
            "--required-token-auth-method".into(),
            "private_key_jwt".into(),
        ],
        [],
    )?;

    let client_assertion_replay_jti = "smoke-client-assertion-replay";
    gateway_token(
        conformance,
        &base,
        &[
            "--scope",
            "media:use",
            "--jwt-id",
            client_assertion_replay_jti,
        ],
    )?;
    if gateway_token(
        conformance,
        &base,
        &[
            "--scope",
            "media:use",
            "--jwt-id",
            client_assertion_replay_jti,
        ],
    )
    .is_ok()
    {
        bail!("replayed private-key JWT client assertion was unexpectedly accepted");
    }

    let http = reqwest::Client::builder()
        .redirect(Policy::none())
        .build()?;
    let code_verifier = "smoke-browser-pkce-verifier-0123456789abcdef0123456789abcdef";
    let code_challenge = "X9AgXux1PHu8RKlqHF9FuDYoLL6yjPFGS5je8BbaBF8";
    let (gateway_code, callback_query) = gateway_browser_authorization_code(
        &http,
        &idp_client,
        &base,
        &idp_base,
        code_challenge,
        "smoke-state",
    )
    .await?;

    let token_response: Value = http
        .post(format!("{base}/oauth/default/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", "veoveo-browser"),
            ("code", gateway_code.as_str()),
            ("redirect_uri", "https://veoveo.bioma.ai/oauth/callback"),
            ("code_verifier", code_verifier),
        ]))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if token_response.get("token_type").and_then(Value::as_str) != Some("Bearer") {
        bail!("authorization-code token response was not bearer: {token_response}");
    }
    let replay_status = http
        .post(format!("{base}/oauth/default/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", "veoveo-browser"),
            ("code", gateway_code.as_str()),
            ("redirect_uri", "https://veoveo.bioma.ai/oauth/callback"),
            ("code_verifier", code_verifier),
        ]))
        .send()
        .await?
        .status();
    if replay_status != StatusCode::BAD_REQUEST {
        bail!("authorization-code replay status was {replay_status}, expected 400");
    }
    let wrong_code_verifier = "smoke-browser-wrong-verifier-0123456789abcdef0123456789abcdef";
    let (wrong_pkce_code, _) = gateway_browser_authorization_code(
        &http,
        &idp_client,
        &base,
        &idp_base,
        code_challenge,
        "smoke-wrong-pkce",
    )
    .await?;
    let wrong_pkce_status = http
        .post(format!("{base}/oauth/default/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", "veoveo-browser"),
            ("code", wrong_pkce_code.as_str()),
            ("redirect_uri", "https://veoveo.bioma.ai/oauth/callback"),
            ("code_verifier", wrong_code_verifier),
        ]))
        .send()
        .await?
        .status();
    if wrong_pkce_status != StatusCode::BAD_REQUEST {
        bail!("wrong PKCE verifier status was {wrong_pkce_status}, expected 400");
    }
    let wrong_pkce_redeem_status = http
        .post(format!("{base}/oauth/default/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", "veoveo-browser"),
            ("code", wrong_pkce_code.as_str()),
            ("redirect_uri", "https://veoveo.bioma.ai/oauth/callback"),
            ("code_verifier", code_verifier),
        ]))
        .send()
        .await?
        .status();
    if wrong_pkce_redeem_status != StatusCode::BAD_REQUEST {
        bail!(
            "wrong-PKCE authorization code remained redeemable with the right verifier: {wrong_pkce_redeem_status}"
        );
    }
    assert_http_status(
        &format!("{base}/oauth/default/callback?{callback_query}"),
        StatusCode::BAD_REQUEST,
    )
    .await?;
    assert_http_post_status(
        &format!("{base}/admin/default/reload-control-plane"),
        None,
        StatusCode::UNAUTHORIZED,
    )
    .await?;

    let admin_token = gateway_token(
        conformance,
        &base,
        &["--scope", "media:use", "--scope", "gateway:admin"],
    )?;
    let revocation = post_json(
        &http,
        &format!("{base}/admin/default/jwt-revocations"),
        Some(admin_token.trim()),
        serde_json::json!({
            "issuer": "https://veoveo.bioma.ai/oauth/default",
            "jwt_id": "smoke-jwt",
            "expires_at": "2999-01-01T00:00:00Z",
            "reason": "smoke"
        }),
    )
    .await?;
    if revocation.get("status").and_then(Value::as_str) != Some("revoked")
        || revocation
            .get("revocation")
            .and_then(|revocation| revocation.get("jwt_id"))
            .and_then(Value::as_str)
            != Some("smoke-jwt")
    {
        bail!("unexpected revocation result: {revocation}");
    }
    let prune = post_json(
        &http,
        &format!("{base}/admin/default/jwt-revocations/prune"),
        Some(admin_token.trim()),
        Value::Null,
    )
    .await?;
    if prune.get("status").and_then(Value::as_str) != Some("pruned")
        || prune.get("deleted").and_then(Value::as_u64) != Some(0)
    {
        bail!("unexpected prune result: {prune}");
    }
    let expired_status = http
        .post(format!("{base}/admin/default/jwt-revocations"))
        .bearer_auth(admin_token.trim())
        .json(&serde_json::json!({
            "issuer": "https://veoveo.bioma.ai/oauth/default",
            "jwt_id": "expired-smoke-jwt",
            "expires_at": "2000-01-01T00:00:00Z",
            "reason": "smoke-expired"
        }))
        .send()
        .await?
        .status();
    if expired_status != StatusCode::BAD_REQUEST {
        bail!("expired JWT revocation status was {expired_status}, expected 400");
    }

    gateway_child.stop();
    let audit_counts = run_gateway_json(gateway, "audit-counts", &state_db)?;
    assert_json_u64_at_least(&audit_counts, "auth_events", 1)?;
    assert_json_u64_at_least(&audit_counts, "policy_events", 1)?;
    let audit_summary = run_gateway_json(gateway, "audit-method-summary", &state_db)?;
    assert_audit_method(&audit_summary, "admin/jwt-revocations", 2, 0)?;
    assert_audit_method(&audit_summary, "admin/jwt-revocations/prune", 1, 0)?;
    assert_audit_method(&audit_summary, "admin/jwt-revocations/result", 2, 0)?;
    assert_audit_method(&audit_summary, "admin/jwt-revocations/prune/result", 1, 0)?;
    let audit_status_summary =
        run_gateway_metadata_summary(gateway, &state_db, "operation_status")?;
    assert_metadata_summary_at_least(&audit_status_summary, "succeeded", 2)?;
    assert_metadata_summary_at_least(&audit_status_summary, "rejected", 1)?;

    idp.stop();
    cleanup.remove_on_drop();
    println!("gateway HTTP smoke ok");
    Ok(())
}

async fn gateway_authenticated(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let media_port = 18801u16;
    let gateway_port = 18802u16;
    let edge_port = 18809u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let edge_base = format!("http://127.0.0.1:{edge_port}");
    let media_log = tmpdir.join("media.log");
    let gateway_log = tmpdir.join("gateway.log");
    let edge_log = tmpdir.join("edge.log");
    let edge_caddyfile = tmpdir.join("Caddyfile");
    let media_state_db = tmpdir.join("media-state.duckdb");
    let gateway_state_db = tmpdir.join("gateway-state.duckdb");

    let mut media_child = spawn_media_s3_smoke(
        media,
        media_port,
        PUBLIC_BASE_URL,
        &media_state_db,
        &media_log,
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;
    let health = reqwest::get(format!("{media_base}/media/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;
    assert_json_log(
        &media_log,
        &[
            ("message", "listening"),
            ("service", "veoveo-media-mcp"),
            ("mcp_path", "/media/mcp"),
        ],
    )?;
    assert_json_log(&media_log, &[("message", "media retention gc completed")])?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, control_plane, &gateway_state_db),
        [
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    assert_ready_profiles(&gateway_base, 1).await?;
    assert_json_log(
        &gateway_log,
        &[("message", "listening"), ("service", "veoveo-mcp-gateway")],
    )?;
    assert_json_log(
        &gateway_log,
        &[("message", "gateway retention gc completed")],
    )?;

    write_edge_caddyfile(&edge_caddyfile, gateway_port, media_port)?;
    let edge_name = format!("veoveo-edge-smoke-{edge_port}-{}", uuid::Uuid::new_v4());
    let _edge_container = ContainerGuard::new(edge_name.clone());
    let mut edge = ChildGuard::spawn(
        Path::new("docker"),
        [
            "run".into(),
            "--rm".into(),
            "--name".into(),
            edge_name.into(),
            "--add-host=host.docker.internal:host-gateway".into(),
            "-p".into(),
            format!("127.0.0.1:{edge_port}:8080").into(),
            "-v".into(),
            format!("{}:/etc/caddy/Caddyfile:ro", edge_caddyfile.display()).into(),
            "caddy:2.11.2".into(),
            "caddy".into(),
            "run".into(),
            "--config".into(),
            "/etc/caddy/Caddyfile".into(),
            "--adapter".into(),
            "caddyfile".into(),
        ],
        [],
        &edge_log,
    )?;
    wait_for_http(&format!("{edge_base}/healthz")).await?;
    contains(
        &reqwest::get(format!("{edge_base}/healthz"))
            .await?
            .error_for_status()?
            .text()
            .await?,
        "ok",
    )?;
    contains(
        &reqwest::get(format!("{edge_base}/media/healthz"))
            .await?
            .error_for_status()?
            .text()
            .await?,
        "ok",
    )?;
    assert_http_status(&format!("{edge_base}/media/mcp"), StatusCode::NOT_FOUND).await?;
    let edge_token = gateway_token(conformance, &edge_base, &["--scope", "media:use"])?;
    run_direct_mcp(
        conformance,
        &format!("{edge_base}/mcp/default"),
        ["info".into()],
        [("MCP_BEARER_TOKEN", edge_token.trim().into())],
    )?;

    let admin_token = gateway_token(
        conformance,
        &gateway_base,
        &["--scope", "media:use", "--scope", "gateway:admin"],
    )?;
    let http = reqwest::Client::new();
    let reload = post_json(
        &http,
        &format!("{gateway_base}/admin/default/reload-control-plane"),
        Some(admin_token.trim()),
        Value::Null,
    )
    .await?;
    let reload_revision_id = assert_control_plane_admin_result(&reload, "reloaded")?;
    if !reload_revision_id.starts_with("gcp-")
        || reload.get("sha256").and_then(Value::as_str).map(str::len) != Some(64)
    {
        bail!("unexpected reload result: {reload}");
    }
    let control_status = get_json(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
    )
    .await?;
    assert_control_plane_status(&control_status, &reload_revision_id)?;

    let applied = put_json_file(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
        control_plane,
    )
    .await?;
    let revision_id = assert_control_plane_admin_result(&applied, "applied")?;
    let control_status = get_json(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
    )
    .await?;
    assert_control_plane_status(&control_status, &revision_id)?;

    let ops_control_plane = tmpdir.join("gateway.ops.json");
    write_ops_profile_control_plane(control_plane, &ops_control_plane)?;
    let ops_applied = put_json_file(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
        &ops_control_plane,
    )
    .await?;
    let ops_revision_id =
        assert_control_plane_admin_result_with_profiles(&ops_applied, "applied", 2)?;
    assert_ready_profiles(&gateway_base, 2).await?;
    let ops_admin_token = gateway_id_jag_token_for_profile(
        conformance,
        &gateway_base,
        "ops",
        &[
            "--id-jag-scope",
            "media:use",
            "--id-jag-scope",
            "gateway:admin",
            "--scope",
            "media:use",
            "--scope",
            "gateway:admin",
        ],
    )?;
    let ops_status = get_json(
        &http,
        &format!("{gateway_base}/admin/ops/control-plane"),
        Some(ops_admin_token.trim()),
    )
    .await?;
    assert_control_plane_status_with_profiles(&ops_status, &ops_revision_id, 2)?;
    let ops_token = gateway_id_jag_token_for_profile(
        conformance,
        &gateway_base,
        "ops",
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    run_direct_mcp(
        conformance,
        &format!("{gateway_base}/mcp/ops"),
        ["info".into()],
        [("MCP_BEARER_TOKEN", ops_token.trim().into())],
    )?;

    let reverted = put_json_file(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
        control_plane,
    )
    .await?;
    let reverted_revision_id = assert_control_plane_admin_result(&reverted, "applied")?;
    assert_ready_profiles(&gateway_base, 1).await?;
    assert_mcp_denied(
        conformance,
        &format!("{gateway_base}/mcp/ops"),
        ops_token.trim(),
        ["info".into()],
    )?;

    gateway_child.stop();
    gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, control_plane, &gateway_state_db),
        [
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    assert_ready_profiles(&gateway_base, 1).await?;
    let control_status = get_json(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
    )
    .await?;
    assert_control_plane_status(&control_status, &reverted_revision_id)?;

    let token = gateway_token(conformance, &gateway_base, &["--scope", "media:use"])?;
    let token = token.trim();
    let gateway_mcp_url = format!("{gateway_base}/mcp/default");
    for args in [
        vec!["info".into()],
        vec!["resources".into()],
        vec!["resource".into(), "media://usage".into()],
        vec!["prompts".into()],
        vec![
            "prompt".into(),
            "media-model-select".into(),
            "--arguments".into(),
            r#"{"goal":"choose an image generation model for a product render","media_type":"image","budget":"low"}"#.into(),
        ],
        vec!["tasks".into()],
    ] {
        run_direct_mcp(
            conformance,
            &gateway_mcp_url,
            args,
            [("MCP_BEARER_TOKEN", token.into())],
        )?;
    }

    let revoked_token = gateway_token(conformance, &gateway_base, &["--scope", "media:use"])?;
    let revoked_jti = jwt_id(revoked_token.trim())?;
    let revocation = post_json(
        &http,
        &format!("{gateway_base}/admin/default/jwt-revocations"),
        Some(admin_token.trim()),
        serde_json::json!({
            "issuer": "https://veoveo.bioma.ai/oauth/default",
            "jwt_id": revoked_jti,
            "expires_at": "2999-01-01T00:00:00Z",
            "reason": "smoke"
        }),
    )
    .await?;
    if revocation.get("status").and_then(Value::as_str) != Some("revoked")
        || revocation
            .get("revocation")
            .and_then(|revocation| revocation.get("jwt_id"))
            .and_then(Value::as_str)
            != Some(revoked_jti.as_str())
    {
        bail!("unexpected JWT revocation result: {revocation}");
    }
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        revoked_token.trim(),
        ["info".into()],
    )?;

    let ema_token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    let ema_token = ema_token.trim();
    run_direct_mcp(
        conformance,
        &gateway_mcp_url,
        ["info".into()],
        [("MCP_BEARER_TOKEN", ema_token.into())],
    )?;
    let live_policy_session = connect_mcp_session(&gateway_mcp_url, token).await?;
    read_mcp_resource_json(&live_policy_session, "media://usage").await?;

    let cui_control_plane = tmpdir.join("gateway.cui.json");
    write_cui_control_plane(control_plane, &cui_control_plane)?;
    let cui_apply = put_json_file(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
        &cui_control_plane,
    )
    .await?;
    assert_control_plane_admin_result(&cui_apply, "applied")?;

    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        token,
        ["resource".into(), "media://usage".into()],
    )?;
    assert_mcp_session_resource_denied(&live_policy_session, "media://usage").await?;
    live_policy_session.cancel().await?;
    let missing_group_token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        missing_group_token.trim(),
        ["resource".into(), "media://usage".into()],
    )?;
    let missing_role_token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--data-label",
            "cui",
        ],
    )?;
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        missing_role_token.trim(),
        ["resource".into(), "media://usage".into()],
    )?;
    run_direct_mcp(
        conformance,
        &gateway_mcp_url,
        ["resource".into(), "media://usage".into()],
        [("MCP_BEARER_TOKEN", ema_token.into())],
    )?;

    let replay_jti = "smoke-id-jag-replay";
    gateway_id_jag_token(
        conformance,
        &gateway_base,
        &["--id-jag-scope", "media:use", "--jwt-id", replay_jti],
    )?;
    if gateway_id_jag_token(
        conformance,
        &gateway_base,
        &["--id-jag-scope", "media:use", "--jwt-id", replay_jti],
    )
    .is_ok()
    {
        bail!("replayed ID-JAG was unexpectedly accepted");
    }

    let denied_token = gateway_token(conformance, &gateway_base, &["--scope", "gateway:admin"])?;
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        denied_token.trim(),
        ["info".into()],
    )?;

    edge.stop();
    gateway_child.stop();
    let audit_counts = run_gateway_json(gateway, "audit-counts", &gateway_state_db)?;
    assert_json_u64_at_least(&audit_counts, "auth_events", 1)?;
    assert_json_u64_at_least(&audit_counts, "policy_events", 1)?;
    let audit_summary = run_gateway_json(gateway, "audit-method-summary", &gateway_state_db)?;
    assert_audit_method(&audit_summary, "tools/list", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/list", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/templates/list", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/read", 3, 3)?;
    assert_audit_method(&audit_summary, "prompts/list", 2, 0)?;
    assert_audit_method(&audit_summary, "prompts/get", 1, 0)?;
    assert_audit_method(&audit_summary, "tasks/list", 1, 0)?;
    let audit_reasons = run_gateway_json(gateway, "audit-reason-summary", &gateway_state_db)?;
    assert_reason_summary_at_least(&audit_reasons, "missing_data_label", 1)?;
    assert_reason_summary_at_least(&audit_reasons, "missing_group", 1)?;
    assert_reason_summary_at_least(&audit_reasons, "missing_role", 1)?;

    media_child.stop();
    cleanup.remove_on_drop();
    println!("gateway authenticated smoke ok");
    Ok(())
}

async fn media_mcp_auth(conformance: &Path, media: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let port = 18800u16;
    let base = format!("http://127.0.0.1:{port}");
    let log = tmpdir.join("media.log");
    let state_db = tmpdir.join("state.duckdb");
    let mut media_child = spawn_media_s3_smoke(media, port, PUBLIC_BASE_URL, &state_db, &log)?;
    wait_for_http(&format!("{base}/media/healthz")).await?;
    let health = reqwest::get(format!("{base}/media/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;
    assert_json_log(
        &log,
        &[
            ("message", "listening"),
            ("service", "veoveo-media-mcp"),
            ("mcp_path", "/media/mcp"),
        ],
    )?;
    assert_json_log(&log, &[("message", "media retention gc completed")])?;
    assert_http_status(&format!("{base}/media/mcp"), StatusCode::UNAUTHORIZED).await?;
    assert_http_status(
        &format!("{base}/media/artifacts/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        StatusCode::UNAUTHORIZED,
    )
    .await?;

    run_checked(
        conformance,
        [
            "--url".into(),
            format!("{base}/media/mcp").into(),
            "info".into(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;

    media_child.stop();
    cleanup.remove_on_drop();
    println!("media MCP auth smoke ok");
    Ok(())
}

async fn media_task_run(conformance: &Path, media: &Path) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let media_port = 18807u16;
    let provider_port = 18808u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let provider_base = format!("http://127.0.0.1:{provider_port}");
    let provider_log = tmpdir.join("provider.log");
    let media_log = tmpdir.join("media.log");
    let provider_ready = tmpdir.join("provider.ready");
    let media_state_db = tmpdir.join("media-state.duckdb");
    let output_dir = tmpdir.join("outputs");

    let mut provider = spawn_fake_media_provider(
        conformance,
        provider_port,
        &provider_ready,
        &provider_log,
        None,
    )?;
    wait_for_file_and_http(&provider_ready, &format!("{provider_base}/api/v3/models")).await?;

    let mut media_child = spawn_media_memory_smoke(
        media,
        media_port,
        &media_base,
        &media_state_db,
        &provider_base,
        &media_log,
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;
    let health = reqwest::get(format!("{media_base}/media/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;

    let mcp_url = format!("{media_base}/media/mcp");
    let cancel_output = run_direct_mcp(
        conformance,
        &mcp_url,
        [
            "run".into(),
            "fake/image".into(),
            "--input".into(),
            r#"{"prompt":"cancel"}"#.into(),
            "--cancel".into(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;
    let cancel_task_id = task_id_from_output(&cancel_output)?;
    contains(
        &cancel_output,
        &format!("cancelled task {cancel_task_id} (status Cancelled)"),
    )?;

    let complete_output = run_direct_mcp(
        conformance,
        &mcp_url,
        ["complete".into(), "fake".into()],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;
    contains(&complete_output, "fake/image")?;

    let run_output = run_direct_mcp(
        conformance,
        &mcp_url,
        [
            "run".into(),
            "fake/image".into(),
            "--input".into(),
            r#"{"prompt":"smoke"}"#.into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
        [("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    )?;
    let task_id = task_id_from_output(&run_output)?;
    let structured: SmokeGenerationRunOutput = structured_from_output(&run_output)?;
    if structured.artifacts.is_empty() {
        bail!("run output had no artifacts: {run_output}");
    }
    if structured.artifacts.iter().any(|artifact| {
        artifact.metadata.get("task_id").and_then(Value::as_str) != Some(task_id.as_str())
    }) {
        bail!("not all artifact metadata rows used task id `{task_id}`: {structured:?}");
    }
    assert_output_file(&output_dir, "png")?;

    let usage = wait_for_actual_usage(conformance, &mcp_url, &task_id, None)?;
    assert_usage_report(&usage, "media", &task_id)?;

    media_child.stop();
    provider.stop();
    cleanup.remove_on_drop();
    println!("media task run smoke ok");
    Ok(())
}

async fn gateway_two_servers(
    conformance: &Path,
    gateway: &Path,
    base_control_plane: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let media_port = 18810u16;
    let simulation_port = 18811u16;
    let gateway_port = 18812u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let simulation_base = format!("http://127.0.0.1:{simulation_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let generated_control_plane = tmpdir.join("gateway.two-server.json");
    let gateway_state_db = tmpdir.join("gateway-state.duckdb");

    let media_log = tmpdir.join("media-fixture.log");
    let simulation_log = tmpdir.join("simulation-fixture.log");
    let gateway_log = tmpdir.join("gateway.log");
    let media_ready = tmpdir.join("media.ready");
    let simulation_ready = tmpdir.join("simulation.ready");

    let mut media = spawn_fake_hosted_mcp(
        conformance,
        media_port,
        "media",
        "media",
        &media_ready,
        &media_log,
    )?;
    let mut simulation = spawn_fake_hosted_mcp(
        conformance,
        simulation_port,
        "simulation",
        "simulation",
        &simulation_ready,
        &simulation_log,
    )?;
    wait_for_file_and_http(&media_ready, &format!("{media_base}/media/healthz")).await?;
    wait_for_file_and_http(
        &simulation_ready,
        &format!("{simulation_base}/simulation/healthz"),
    )
    .await?;

    run_checked(
        conformance,
        [
            "gateway-two-server-smoke-control-plane".into(),
            "--base".into(),
            base_control_plane.as_os_str().to_os_string(),
            "--output".into(),
            generated_control_plane.as_os_str().to_os_string(),
            "--media-upstream-url".into(),
            format!("{media_base}/media/mcp").into(),
            "--simulation-upstream-url".into(),
            format!("{simulation_base}/simulation/mcp").into(),
        ],
        [],
    )?;
    let validation = run_checked(
        gateway,
        [
            "validate".into(),
            "--control-plane".into(),
            generated_control_plane.as_os_str().to_os_string(),
        ],
        [],
    )?;
    contains(&validation, "ok: 2 server(s), 1 profile(s)")?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        [
            "serve".into(),
            "--port".into(),
            gateway_port.to_string().into(),
            "--public-base-url".into(),
            PUBLIC_BASE_URL.into(),
            "--control-plane".into(),
            generated_control_plane.as_os_str().to_os_string(),
            "--state-db".into(),
            gateway_state_db.as_os_str().to_os_string(),
        ],
        [
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    let ready: Value = reqwest::get(format!("{gateway_base}/readyz"))
        .await?
        .error_for_status()?
        .json()
        .await?;
    if ready.get("servers").and_then(Value::as_u64) != Some(2) {
        bail!("gateway readyz did not report two servers: {ready}");
    }

    let token = run_checked(
        conformance,
        [
            "gateway-token-exchange".into(),
            "--token-url".into(),
            format!("{gateway_base}/oauth/default/token").into(),
            "--scope".into(),
            "media:use".into(),
            "--scope".into(),
            "simulation:use".into(),
        ],
        [],
    )?;
    let token = token.trim();
    let info = run_mcp(conformance, &gateway_base, token, ["info".into()])?;
    for expected in [
        "tool `media__run`",
        "tool `simulation__run`",
        "prompt `media-plan`",
        "prompt `simulation-plan`",
        "template: media://scenario/{scenario_id}",
        "template: simulation://scenario/{scenario_id}",
    ] {
        contains(&info, expected)?;
    }

    let media_call = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "call".into(),
            "--tool-name".into(),
            "media__run".into(),
            "--arguments".into(),
            r#"{"scenario":"supply-chain"}"#.into(),
        ],
    )?;
    contains(&media_call, "media fixture accepted scenario supply-chain")?;
    assert_structured_field(&media_call, "server", "media")?;
    assert_structured_field(&media_call, "scenario_uri", "media://scenario/supply-chain")?;

    let simulation_call = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "call".into(),
            "--tool-name".into(),
            "simulation__run".into(),
            "--arguments".into(),
            r#"{"scenario":"orbital-docking"}"#.into(),
        ],
    )?;
    contains(
        &simulation_call,
        "simulation fixture accepted scenario orbital-docking",
    )?;
    assert_structured_field(&simulation_call, "server", "simulation")?;
    assert_structured_field(
        &simulation_call,
        "scenario_uri",
        "simulation://scenario/orbital-docking",
    )?;

    let resource = run_mcp(
        conformance,
        &gateway_base,
        token,
        ["resource".into(), "simulation://scenarios".into()],
    )?;
    let resource: Value = serde_json::from_str(&resource)?;
    if resource.get("server").and_then(Value::as_str) != Some("simulation")
        || resource
            .get("scenarios")
            .and_then(Value::as_array)
            .map(Vec::len)
            != Some(3)
    {
        bail!("simulation resource was not routed correctly: {resource}");
    }

    let prompt = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "prompt".into(),
            "simulation-plan".into(),
            "--arguments".into(),
            r#"{"scenario":"orbital-docking"}"#.into(),
        ],
    )?;
    contains(&prompt, "simulation fixture plan")?;

    let completion = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "complete-resource".into(),
            "--uri".into(),
            "simulation://scenario/{scenario_id}".into(),
            "--argument".into(),
            "scenario_id".into(),
            "orb".into(),
        ],
    )?;
    contains(&completion, "orbital-docking")?;

    let media_only_token = run_checked(
        conformance,
        [
            "gateway-token-exchange".into(),
            "--token-url".into(),
            format!("{gateway_base}/oauth/default/token").into(),
            "--scope".into(),
            "media:use".into(),
        ],
        [],
    )?;
    let denied = run_raw(
        conformance,
        [
            "--url".into(),
            format!("{gateway_base}/mcp/default").into(),
            "call".into(),
            "--tool-name".into(),
            "simulation__run".into(),
            "--arguments".into(),
            r#"{"scenario":"orbital-docking"}"#.into(),
        ],
        [("MCP_BEARER_TOKEN", media_only_token.trim().into())],
    )?;
    if denied.status.success() {
        bail!("media-only token unexpectedly called simulation tool");
    }

    gateway_child.stop();
    let audit_summary = run_checked(
        gateway,
        [
            "audit-method-summary".into(),
            "--state-db".into(),
            gateway_state_db.as_os_str().to_os_string(),
        ],
        [],
    )?;
    let audit_summary: Value = serde_json::from_str(&audit_summary)?;
    assert_audit_method(&audit_summary, "tools/call", 2, 1)?;
    assert_audit_method(&audit_summary, "resources/read", 1, 0)?;
    assert_audit_method(&audit_summary, "prompts/get", 1, 0)?;
    assert_audit_method(&audit_summary, "completion/complete", 1, 0)?;

    media.stop();
    simulation.stop();
    cleanup.remove_on_drop();
    println!("gateway two-server smoke ok");
    Ok(())
}

async fn gateway_task_run(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let media_port = 18801u16;
    let gateway_port = 18802u16;
    let provider_port = 18806u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let provider_base = format!("http://127.0.0.1:{provider_port}");
    let provider_log = tmpdir.join("provider.log");
    let media_log = tmpdir.join("media.log");
    let gateway_log = tmpdir.join("gateway.log");
    let provider_ready = tmpdir.join("provider.ready");
    let media_state_db = tmpdir.join("media-state.duckdb");
    let gateway_state_db = tmpdir.join("gateway-state.duckdb");
    let output_dir = tmpdir.join("outputs");

    let mut provider = spawn_fake_media_provider(
        conformance,
        provider_port,
        &provider_ready,
        &provider_log,
        Some(4000),
    )?;
    wait_for_file_and_http(&provider_ready, &format!("{provider_base}/api/v3/models")).await?;

    let mut media_child = spawn_media_memory_smoke(
        media,
        media_port,
        &media_base,
        &media_state_db,
        &provider_base,
        &media_log,
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, control_plane, &gateway_state_db),
        [
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.trim().into(),
            ),
        ],
        &gateway_log,
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    assert_ready_profiles(&gateway_base, 1).await?;

    let token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    let token = token.trim();

    let cancel_output = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "run".into(),
            "fake/image".into(),
            "--tool-name".into(),
            "media__run".into(),
            "--input".into(),
            r#"{"prompt":"cancel"}"#.into(),
            "--cancel".into(),
        ],
    )?;
    let cancel_task_id = task_id_from_output(&cancel_output)?;
    contains(
        &cancel_output,
        &format!("cancelled task {cancel_task_id} (status Cancelled)"),
    )?;
    contains(&cancel_output, "  [resource list changed]")?;
    contains(
        &cancel_output,
        &format!("  [task {cancel_task_id}] Working: submitted; prediction"),
    )?;

    let complete_output = run_mcp(
        conformance,
        &gateway_base,
        token,
        ["complete".into(), "fake".into()],
    )?;
    contains(&complete_output, "fake/image")?;

    let run_output = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "run".into(),
            "fake/image".into(),
            "--tool-name".into(),
            "media__run".into(),
            "--input".into(),
            r#"{"prompt":"smoke"}"#.into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
    )?;
    let task_id = task_id_from_output(&run_output)?;
    for expected in [
        "  [resource list changed]".to_string(),
        format!("  [task {task_id}] Working: submitted; prediction"),
        "  [resource updated] media://prediction/".to_string(),
        format!("  [task {task_id}] Completed: completed;"),
        "subscribed to media://prediction/".to_string(),
        "unsubscribed from media://prediction/".to_string(),
    ] {
        contains(&run_output, &expected)?;
    }

    let structured: SmokeGenerationRunOutput = structured_from_output(&run_output)?;
    if structured.artifacts.is_empty() {
        bail!("run output had no artifacts: {run_output}");
    }
    for artifact in &structured.artifacts {
        if artifact.metadata.get("task_id").and_then(Value::as_str) != Some(task_id.as_str()) {
            bail!("artifact metadata did not use task id `{task_id}`: {artifact:?}");
        }
        if artifact.compliance.tenant_id.as_deref() != Some("tenant-a")
            || !artifact
                .compliance
                .data_labels
                .iter()
                .any(|label| label == "cui")
        {
            bail!("artifact compliance labels were not propagated: {artifact:?}");
        }
    }
    assert_output_file(&output_dir, "png")?;

    let usage = wait_for_actual_usage(
        conformance,
        &format!("{gateway_base}/mcp/default"),
        &task_id,
        Some(token),
    )?;
    assert_usage_report(&usage, "media", &task_id)?;

    gateway_child.stop();
    let audit_summary = run_checked(
        gateway,
        [
            "audit-method-summary".into(),
            "--state-db".into(),
            gateway_state_db.as_os_str().to_os_string(),
        ],
        [],
    )?;
    let audit_summary: Value = serde_json::from_str(&audit_summary)?;
    assert_no_audit_denies(&audit_summary)?;
    assert_audit_method(&audit_summary, "completion/complete", 1, 0)?;
    assert_audit_method(&audit_summary, "tools/call", 2, 0)?;
    assert_audit_method(&audit_summary, "tasks/cancel", 1, 0)?;
    assert_audit_method(&audit_summary, "tasks/get", 2, 0)?;
    assert_audit_method(&audit_summary, "tasks/result", 2, 0)?;
    assert_audit_method(&audit_summary, "resources/subscribe", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/unsubscribe", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/read", 2, 0)?;

    media_child.stop();
    provider.stop();
    cleanup.remove_on_drop();
    println!("gateway task run smoke ok");
    Ok(())
}

fn spawn_fake_hosted_mcp(
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

fn spawn_fake_media_provider(
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

fn spawn_media_s3_smoke(
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

fn spawn_media_memory_smoke(
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

fn write_edge_caddyfile(path: &Path, gateway_port: u16, media_port: u16) -> Result<()> {
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

fn gateway_serve_args(port: u16, control_plane: &Path, state_db: &Path) -> Vec<OsString> {
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

fn assert_executable(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("required binary does not exist: {}", path.display());
    }
    Ok(())
}

async fn wait_for_file_and_http(file: &Path, url: &str) -> Result<()> {
    for _ in 0..150 {
        if file.exists() && http_ok(url).await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {url} and {}", file.display());
}

async fn wait_for_file(file: &Path) -> Result<()> {
    for _ in 0..150 {
        if file.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {}", file.display());
}

async fn wait_for_file_contains(file: &Path, first: &str, second: &str) -> Result<()> {
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

async fn wait_for_http(url: &str) -> Result<()> {
    for _ in 0..150 {
        if http_ok(url).await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {url}");
}

async fn wait_for_http_client(
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

async fn http_ok(url: &str) -> Result<bool> {
    let response = reqwest::get(url).await;
    Ok(matches!(response, Ok(response) if response.status() == StatusCode::OK))
}

async fn assert_http_status(url: &str, expected: StatusCode) -> Result<()> {
    let status = reqwest::get(url).await?.status();
    if status == expected {
        Ok(())
    } else {
        bail!("expected {expected} from {url}, got {status}");
    }
}

async fn assert_http_post_status(
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

async fn assert_ready_profiles(gateway_base: &str, expected: u64) -> Result<()> {
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

fn run_mcp(
    conformance: &Path,
    gateway_base: &str,
    token: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let mut all_args = vec!["--url".into(), format!("{gateway_base}/mcp/default").into()];
    all_args.extend(args);
    run_checked(conformance, all_args, [("MCP_BEARER_TOKEN", token.into())])
}

async fn connect_mcp_session(url: &str, bearer_token: &str) -> Result<SmokeMcpSession> {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url.to_string())
            .auth_header(bearer_token.to_string()),
    );
    Ok(SmokeMcpClient.serve(transport).await?)
}

async fn read_mcp_resource_json(session: &SmokeMcpSession, uri: &str) -> Result<Value> {
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

async fn assert_mcp_session_resource_denied(session: &SmokeMcpSession, uri: &str) -> Result<()> {
    if read_mcp_resource_json(session, uri).await.is_ok() {
        bail!("same MCP session unexpectedly read `{uri}` after policy update");
    }
    Ok(())
}

fn gateway_id_jag_token(conformance: &Path, gateway_base: &str, args: &[&str]) -> Result<String> {
    gateway_id_jag_token_for_profile(conformance, gateway_base, "default", args)
}

fn gateway_id_jag_token_for_profile(
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

fn gateway_token(conformance: &Path, gateway_base: &str, args: &[&str]) -> Result<String> {
    let mut all_args = vec![
        "gateway-token-exchange".into(),
        "--token-url".into(),
        format!("{gateway_base}/oauth/default/token").into(),
    ];
    all_args.extend(args.iter().map(|arg| OsString::from(*arg)));
    run_checked(conformance, all_args, [])
}

fn run_gateway_json(gateway: &Path, command: &str, state_db: &Path) -> Result<Value> {
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

fn run_gateway_metadata_summary(
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

fn run_direct_mcp(
    conformance: &Path,
    url: &str,
    args: impl IntoIterator<Item = OsString>,
    envs: impl IntoIterator<Item = (&'static str, OsString)>,
) -> Result<String> {
    let mut all_args = vec!["--url".into(), url.into()];
    all_args.extend(args);
    run_checked(conformance, all_args, envs)
}

fn run_checked(
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

fn run_raw(
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

fn configure_binary_runtime(command: &mut Command, program: &Path) {
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

fn prepend_path_env(command: &mut Command, key: &str, path: &Path) {
    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = env::var_os(key) {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(paths) {
        command.env(key, joined);
    }
}

fn contains(haystack: &str, needle: &str) -> Result<()> {
    if haystack.contains(needle) {
        Ok(())
    } else {
        bail!("expected output to contain `{needle}`\noutput:\n{haystack}");
    }
}

fn https_client_with_ca(cert_path: &Path) -> Result<reqwest::Client> {
    let cert = reqwest::Certificate::from_pem(&fs::read(cert_path)?)?;
    Ok(reqwest::Client::builder()
        .add_root_certificate(cert)
        .redirect(Policy::none())
        .build()?)
}

fn redirect_location(response: reqwest::Response, expected: StatusCode) -> Result<String> {
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

async fn gateway_browser_authorization_code(
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

fn url_query_value(url: &str, key: &str) -> Result<String> {
    let url = reqwest::Url::parse(url)?;
    url.query_pairs()
        .find_map(|(query_key, value)| (query_key == key).then(|| value.into_owned()))
        .ok_or_else(|| anyhow!("URL had no `{key}` query value: {url}"))
}

fn form_urlencoded(fields: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(fields.iter().copied());
    serializer.finish()
}

async fn post_json(
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

async fn get_json(
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

async fn put_json_file(
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

fn assert_control_plane_admin_result(value: &Value, expected_status: &str) -> Result<String> {
    assert_control_plane_admin_result_with_profiles(value, expected_status, 1)
}

fn assert_control_plane_admin_result_with_profiles(
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

fn assert_control_plane_status(value: &Value, expected_revision_id: &str) -> Result<()> {
    assert_control_plane_status_with_profiles(value, expected_revision_id, 1)
}

fn assert_control_plane_status_with_profiles(
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

fn jwt_id(token: &str) -> Result<String> {
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

fn assert_mcp_denied(
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

fn write_cui_control_plane(input: &Path, output: &Path) -> Result<()> {
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

fn write_ops_profile_control_plane(input: &Path, output: &Path) -> Result<()> {
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

fn append_unique_string(value: &mut Value, key: &str, item: &str) -> Result<()> {
    let values = value
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("JSON object has no `{key}` array"))?;
    if !values.iter().any(|value| value.as_str() == Some(item)) {
        values.push(Value::String(item.to_string()));
    }
    Ok(())
}

fn assert_schema_title(path: &Path, expected_title: &str) -> Result<Value> {
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

fn assert_json_log(path: &Path, expected: &[(&str, &str)]) -> Result<()> {
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

fn smoke_tmpdir() -> Result<PathBuf> {
    let tmpdir = env::temp_dir().join(format!("veoveo-smoke-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmpdir)?;
    Ok(tmpdir)
}

fn assert_structured_field(output: &str, field: &str, expected: &str) -> Result<()> {
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

fn task_id_from_output(output: &str) -> Result<String> {
    output
        .lines()
        .find_map(|line| {
            line.strip_prefix("task ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_string)
        })
        .ok_or_else(|| anyhow!("command output had no task id:\n{output}"))
}

fn structured_from_output<T: DeserializeOwned>(output: &str) -> Result<T> {
    let structured = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .ok_or_else(|| anyhow!("command output had no structured content:\n{output}"))?;
    Ok(serde_json::from_str(structured)?)
}

fn wait_for_actual_usage(
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

fn usage_envs(bearer_token: Option<&str>) -> Vec<(&'static str, OsString)> {
    match bearer_token {
        Some(token) => vec![("MCP_BEARER_TOKEN", token.into())],
        None => vec![("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into())],
    }
}

fn assert_usage_report(report: &SmokeUsageReport, scheme: &str, task_id: &str) -> Result<()> {
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

fn assert_output_file(output_dir: &Path, extension: &str) -> Result<()> {
    if contains_nonempty_file_with_extension(output_dir, extension)? {
        Ok(())
    } else {
        bail!(
            "no non-empty .{extension} output file found under {}",
            output_dir.display()
        );
    }
}

fn contains_nonempty_file_with_extension(path: &Path, extension: &str) -> Result<bool> {
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

fn assert_audit_method(summary: &Value, method: &str, min_allow: u64, min_deny: u64) -> Result<()> {
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

fn assert_json_u64_at_least(value: &Value, key: &str, minimum: u64) -> Result<()> {
    let actual = value.get(key).and_then(Value::as_u64).unwrap_or_default();
    if actual >= minimum {
        Ok(())
    } else {
        bail!("JSON field `{key}` was {actual}, expected at least {minimum}: {value}");
    }
}

fn assert_metadata_summary_at_least(
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

fn assert_reason_summary_at_least(summary: &Value, reason: &str, minimum: u64) -> Result<()> {
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

fn assert_no_audit_denies(summary: &Value) -> Result<()> {
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

struct TmpDirGuard {
    path: PathBuf,
    remove_on_drop: bool,
}

impl TmpDirGuard {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            remove_on_drop: false,
        }
    }

    fn remove_on_drop(&mut self) {
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
