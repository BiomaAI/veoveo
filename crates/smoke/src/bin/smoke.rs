use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use reqwest::StatusCode;
use serde_json::Value;

const INTERNAL_SECRET: &str = "local-smoke-internal-token-secret-32-bytes-minimum";
const PUBLIC_BASE_URL: &str = "https://veoveo.bioma.ai";

#[derive(Parser, Debug)]
#[command(name = "smoke", about = "Veoveo smoke-test harness")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    match args.cmd {
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
        Cmd::GatewayTwoServers {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_two_servers(&conformance_bin, &gateway_bin, &control_plane).await,
    }
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
