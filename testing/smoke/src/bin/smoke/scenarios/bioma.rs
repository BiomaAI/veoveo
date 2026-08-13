use std::collections::BTreeSet;

use anyhow::ensure;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};

use super::*;

const NAMESPACE: &str = "veoveo";
const LARGE_ARTIFACT_ROWS: u64 = 200_000;
const LARGE_ARTIFACT_MINIMUM_BYTES: usize = 8 * 1024 * 1024;
const OPERATOR_PROFILE_SCOPES: &[&str] = &[
    "operator:use",
    "uav-sim:stream",
    "view:read",
    "view:write",
    "view:capture",
    "map:dataset:read",
    "time:read",
];
const BIOMA_DEPLOYMENTS: &[&str] = &[
    "mcp-gateway",
    "artifact-service",
    "console-bff",
    "recording",
    "artifact-mcp",
    "media-mcp",
    "stream-mcp",
    "reason-mcp",
    "timeseries-mcp",
    "duckdb-mcp",
    "optimization-mcp",
    "frames-mcp",
    "map-mcp",
    "view-mcp",
    "time-mcp",
    "datasheet-mcp",
    "chart-mcp",
    "uav-sim",
    "rerun-bridge",
    "cloudflared",
];

pub(crate) async fn bioma_verify(
    conformance: &Path,
    context: &str,
    local_base_url: &str,
    public_base_url: &str,
) -> Result<()> {
    assert_executable(conformance)?;
    run_checked(
        Path::new("kubectl"),
        ["--context", context, "cluster-info"].map(OsString::from),
        [],
    )
    .with_context(|| format!("Kubernetes context {context} is unavailable"))?;

    for deployment in BIOMA_DEPLOYMENTS {
        assert_available_deployment(context, deployment)?;
    }
    assert_gpu_capacity(context, 6)?;

    let public = url::Url::parse(public_base_url).context("parsing public Bioma URL")?;
    ensure!(
        public.scheme() == "https",
        "public Bioma URL must use HTTPS"
    );
    let public_host = public
        .host_str()
        .context("public Bioma URL must include a host")?;
    let local = url::Url::parse(local_base_url).context("parsing local Bioma URL")?;
    ensure!(
        local.scheme() == "http" && local.host_str().is_some_and(is_loopback_host),
        "local Bioma URL must use loopback HTTP"
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    wait_for_health(&client, local_base_url, Some(public_host), 30).await?;
    wait_for_health(&client, public_base_url, None, 150).await?;
    verify_public_console(public_base_url).await?;

    let jwks_url = format!("{}/oauth/jwks.json", public_base_url.trim_end_matches('/'));
    let jwks: Value = client
        .get(&jwks_url)
        .send()
        .await
        .context("requesting the public Bioma JWKS")?
        .error_for_status()
        .context("public Bioma JWKS returned an error")?
        .json()
        .await
        .context("decoding the public Bioma JWKS")?;
    ensure!(
        jwks.get("keys")
            .and_then(Value::as_array)
            .is_some_and(|keys| {
                keys.iter().any(|key| {
                    key.get("kid").and_then(Value::as_str) == Some("veoveo-bioma-2026-07")
                })
            }),
        "public endpoint did not expose the Bioma authorization-server key"
    );
    verify_large_artifact_delivery(conformance, public_base_url).await?;

    println!(
        "Bioma verify ok: the full server catalog is available, both Isaac renderers, View, Stream, and Reason are concurrently schedulable, the single public origin serves console and authorization surfaces, the Bioma JWKS is authoritative, and a deterministic large governed artifact passed full, HEAD, and ranged streaming without a redirect"
    );
    Ok(())
}

async fn verify_large_artifact_delivery(conformance: &Path, public_base_url: &str) -> Result<()> {
    let base = public_base_url.trim_end_matches('/');
    let token = gateway_token_for_context(
        conformance,
        base,
        "operator-service",
        "operator",
        OPERATOR_PROFILE_SCOPES,
        "operations",
    )
    .await?;

    let execute = run_public_conformance(
        conformance,
        base,
        &token,
        &[
            "call",
            "--tool-name",
            "duckdb__execute",
            "--arguments",
            r#"{"db":"artifact_delivery_acceptance","sql":"CREATE OR REPLACE TABLE marker AS SELECT 1 AS ready","create_if_missing":true}"#,
        ],
        Duration::from_secs(60),
    )
    .await?;
    let execute = structured_output(&execute)?;
    ensure!(
        execute.get("db").and_then(Value::as_str) == Some("artifact_delivery_acceptance"),
        "large-artifact setup returned an unexpected DuckDB identity: {execute}"
    );

    let export_sql = format!(
        "SELECT i, sha256(CAST(i AS VARCHAR)) AS digest FROM range({LARGE_ARTIFACT_ROWS}) AS t(i) ORDER BY i"
    );
    let arguments = serde_json::to_string(&serde_json::json!({
        "db": "artifact_delivery_acceptance",
        "selection": {
            "kind": "sql",
            "sql": export_sql,
        },
        "format": "csv",
    }))?;
    let export = run_public_conformance(
        conformance,
        base,
        &token,
        &[
            "task-call",
            "--tool-name",
            "duckdb__export",
            "--arguments",
            &arguments,
            "--timeout-seconds",
            "180",
        ],
        Duration::from_secs(210),
    )
    .await?;
    let export = structured_output(&export)?;
    ensure!(
        export.get("rows_exported").and_then(Value::as_u64) == Some(LARGE_ARTIFACT_ROWS),
        "large-artifact export returned an unexpected row count: {export}"
    );
    let artifact = export
        .get("artifact")
        .and_then(Value::as_object)
        .context("large-artifact export omitted typed artifact metadata")?;
    let artifact_id = artifact
        .get("artifact_id")
        .and_then(Value::as_str)
        .context("large-artifact export omitted artifact_id")?;
    veoveo_mcp_contract::ArtifactId::parse(artifact_id)
        .context("large-artifact export returned an invalid artifact_id")?;
    ensure!(
        !artifact.contains_key("download_url"),
        "artifact metadata must not expose storage download plumbing: {artifact:?}"
    );

    let expected = expected_large_artifact();
    ensure!(
        expected.len() > LARGE_ARTIFACT_MINIMUM_BYTES,
        "large-artifact fixture must remain larger than 8 MiB"
    );
    ensure!(
        artifact.get("byte_len").and_then(Value::as_u64) == Some(expected.len() as u64),
        "artifact metadata byte length does not match deterministic export"
    );
    let expected_digest = Sha256::digest(&expected);
    let download_url = format!("{base}/artifacts/operator/{artifact_id}/download");
    let public_origin = url::Url::parse(base)?.origin();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .redirect(Policy::none())
        .build()?;

    let full = client
        .get(&download_url)
        .bearer_auth(&token)
        .send()
        .await
        .context("downloading the large artifact through the public origin")?;
    assert_artifact_response(
        &full,
        StatusCode::OK,
        &public_origin,
        expected.len() as u64,
        None,
    )?;
    let full_bytes = full.bytes().await?;
    ensure!(
        full_bytes.as_ref() == expected.as_slice()
            && Sha256::digest(&full_bytes) == expected_digest,
        "full public artifact download failed deterministic content and SHA-256 verification"
    );

    let head = client
        .head(&download_url)
        .bearer_auth(&token)
        .send()
        .await
        .context("requesting large artifact metadata through the public origin")?;
    assert_artifact_response(
        &head,
        StatusCode::OK,
        &public_origin,
        expected.len() as u64,
        None,
    )?;
    ensure!(
        head.bytes().await?.is_empty(),
        "artifact HEAD response transferred a body"
    );

    let range_start = LARGE_ARTIFACT_MINIMUM_BYTES;
    let range_end = range_start + 1023;
    let range = client
        .get(&download_url)
        .bearer_auth(&token)
        .header(
            reqwest::header::RANGE,
            format!("bytes={range_start}-{range_end}"),
        )
        .send()
        .await
        .context("requesting a large artifact byte range through the public origin")?;
    assert_artifact_response(
        &range,
        StatusCode::PARTIAL_CONTENT,
        &public_origin,
        (range_end - range_start + 1) as u64,
        Some(&format!(
            "bytes {range_start}-{range_end}/{}",
            expected.len()
        )),
    )?;
    ensure!(
        range.bytes().await?.as_ref() == &expected[range_start..=range_end],
        "public artifact byte range did not match the deterministic export"
    );
    Ok(())
}

fn assert_artifact_response(
    response: &reqwest::Response,
    expected_status: StatusCode,
    public_origin: &url::Origin,
    expected_length: u64,
    expected_range: Option<&str>,
) -> Result<()> {
    ensure!(
        response.status() == expected_status,
        "public artifact response returned {}, expected {expected_status}",
        response.status()
    );
    ensure!(
        response.url().origin() == *public_origin,
        "public artifact response escaped the installation origin: {}",
        response.url()
    );
    ensure!(
        !response.headers().contains_key(LOCATION),
        "public artifact response exposed a redirect"
    );
    ensure!(
        response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            == Some(expected_length),
        "public artifact response returned an incorrect Content-Length"
    );
    ensure!(
        response
            .headers()
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|value| value.to_str().ok())
            == Some("bytes"),
        "public artifact response omitted Accept-Ranges: bytes"
    );
    let content_range = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok());
    ensure!(
        content_range == expected_range,
        "public artifact response returned Content-Range {content_range:?}, expected {expected_range:?}"
    );
    Ok(())
}

async fn run_public_conformance(
    conformance: &Path,
    base: &str,
    token: &str,
    operation: &[&str],
    timeout: Duration,
) -> Result<String> {
    let url = format!("{base}/mcp/operator");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args(["--url", &url])
        .args(operation)
        .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")
        .env("MCP_BEARER_TOKEN", token)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .with_context(|| format!("public conformance operation {operation:?} timed out"))??;
    ensure!(
        output.status.success(),
        "public conformance operation {operation:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).context("decoding public conformance output")
}

fn structured_output(output: &str) -> Result<Value> {
    let encoded = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .with_context(|| format!("conformance output omitted structured content:\n{output}"))?;
    serde_json::from_str(encoded).context("decoding structured MCP output")
}

fn expected_large_artifact() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(15 * 1024 * 1024);
    bytes.extend_from_slice(b"i,digest\n");
    for index in 0..LARGE_ARTIFACT_ROWS {
        let decimal = index.to_string();
        bytes.extend_from_slice(decimal.as_bytes());
        bytes.push(b',');
        bytes.extend_from_slice(hex::encode(Sha256::digest(decimal.as_bytes())).as_bytes());
        bytes.push(b'\n');
    }
    bytes
}

async fn verify_public_console(public_base_url: &str) -> Result<()> {
    let base = url::Url::parse(public_base_url).context("parsing public console base URL")?;
    let browser = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .cookie_store(true)
        .redirect(Policy::none())
        .build()?;

    let root = browser
        .get(base.clone())
        .send()
        .await
        .context("requesting the public Bioma root")?;
    ensure!(
        root.status() == StatusCode::PERMANENT_REDIRECT
            && root
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                == Some("/console/"),
        "public Bioma root must redirect permanently to /console/"
    );

    let console_url = base.join("/console/")?;
    let console = browser
        .get(console_url)
        .send()
        .await
        .context("requesting the public Bioma console")?
        .error_for_status()
        .context("public Bioma console returned an error")?;
    let html = console.text().await?;
    let document = Html::parse_document(&html);
    let selector = Selector::parse("script[src], link[href]")
        .map_err(|error| anyhow!("building console asset selector: {error}"))?;
    let asset_paths = document
        .select(&selector)
        .filter_map(|element| {
            element
                .value()
                .attr("src")
                .or_else(|| element.value().attr("href"))
        })
        .filter(|path| path.starts_with("/console/"))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    ensure!(
        asset_paths.iter().any(|path| path.ends_with(".js"))
            && asset_paths.iter().any(|path| path.ends_with(".css"))
            && asset_paths.contains("/console/favicon.svg"),
        "public console HTML must reference JavaScript, CSS, and favicon assets under /console/"
    );
    for path in asset_paths {
        browser
            .get(base.join(&path)?)
            .send()
            .await
            .with_context(|| format!("requesting public console asset {path}"))?
            .error_for_status()
            .with_context(|| format!("public console asset {path} returned an error"))?;
    }

    let login = browser
        .get(base.join("/auth/login")?)
        .send()
        .await
        .context("starting public console authorization")?;
    ensure!(
        login.status() == StatusCode::SEE_OTHER,
        "console login must redirect to the Veoveo authorization endpoint"
    );
    let authorize_location = login
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .context("console login omitted its authorization redirect")?;
    let authorize = browser
        .get(base.join(authorize_location)?)
        .send()
        .await
        .context("requesting the Veoveo authorization endpoint")?;
    ensure!(
        authorize.status() == StatusCode::FOUND,
        "Veoveo authorization must redirect to the external identity provider"
    );
    let identity_provider = authorize
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .context("Veoveo authorization omitted the identity-provider redirect")?;
    let identity_provider = url::Url::parse(identity_provider)?;
    ensure!(
        identity_provider.scheme() == "https"
            && identity_provider.host_str() == Some("login.microsoftonline.com"),
        "Bioma console authorization must continue at Microsoft Entra"
    );
    Ok(())
}

fn assert_available_deployment(context: &str, deployment: &str) -> Result<()> {
    let output = run_checked(
        Path::new("kubectl"),
        [
            "--context",
            context,
            "--namespace",
            NAMESPACE,
            "get",
            "deployment",
            deployment,
            "--output",
            "jsonpath={.status.availableReplicas}",
        ]
        .map(OsString::from),
        [],
    )?;
    let available = output.trim().parse::<u32>().unwrap_or_default();
    ensure!(
        available > 0,
        "deployment {deployment} has no available replicas in {context}"
    );
    Ok(())
}

fn assert_gpu_capacity(context: &str, minimum: u32) -> Result<()> {
    let output = run_checked(
        Path::new("kubectl"),
        [
            "--context",
            context,
            "get",
            "nodes",
            "--output",
            "jsonpath={range .items[*]}{.status.allocatable.nvidia\\.com/gpu}{\"\\n\"}{end}",
        ]
        .map(OsString::from),
        [],
    )?;
    let capacity = output
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .sum::<u32>();
    ensure!(
        capacity >= minimum,
        "the reference profile requires at least {minimum} allocatable NVIDIA GPU shares; {context} reports {capacity}"
    );
    Ok(())
}

async fn wait_for_health(
    client: &reqwest::Client,
    base_url: &str,
    host_header: Option<&str>,
    attempts: usize,
) -> Result<()> {
    let url = format!("{}/healthz", base_url.trim_end_matches('/'));
    let mut last = String::from("no response");
    for _ in 0..attempts {
        let mut request = client.get(&url);
        if let Some(host) = host_header {
            request = request.header(HOST, host);
        }
        match request.send().await {
            Ok(response) if response.status() == StatusCode::OK => {
                let body = response.text().await?;
                ensure!(body.trim() == "ok", "unexpected health body from {url}");
                return Ok(());
            }
            Ok(response) => last = format!("HTTP {}", response.status()),
            Err(error) => last = error.to_string(),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    bail!("{url} did not become healthy after {attempts} attempts: {last}")
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}
