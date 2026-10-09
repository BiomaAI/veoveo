use super::*;
use anyhow::ensure;
use rmcp::model::{GetTaskParams, ServerNotification, SubscriptionFilter, TaskStatus};
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use veoveo_duckdb_mcp::{
    DuckDbDatabaseId, DuckDbExecuteOutput, DuckDbExecuteRequest, DuckDbExportOutput,
    DuckDbExportRequest, DuckDbTabularFormat, DuckDbTabularSelection,
};
const LARGE_ARTIFACT_ROWS: u64 = 200_000;

const LARGE_ARTIFACT_MINIMUM_BYTES: usize = 8 * 1024 * 1024;

pub(crate) async fn installation_verify(
    conformance: &Path,
    installation: &InstalledTarget,
) -> Result<()> {
    let target = &installation.target;
    let context = &target.kubernetes.context;
    let local_base_url = target.local_base_url.as_str().trim_end_matches('/');
    let public_base_url = installation.public_base();
    assert_executable(conformance)?;
    run_checked(
        Path::new("kubectl"),
        [
            "--request-timeout=30s",
            "--context",
            context,
            "cluster-info",
        ]
        .map(OsString::from),
        [],
    )
    .with_context(|| format!("Kubernetes context {context} is unavailable"))?;

    for deployment in &target.expected_deployments {
        assert_available_deployment(context, &target.kubernetes.namespace, deployment)?;
    }
    assert_gpu_capacity(context, target.minimum_gpu_shares)?;

    let public_host = target
        .public_base_url
        .host_str()
        .context("public installation URL must include a host")?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    wait_for_health(&client, local_base_url, Some(public_host), 30).await?;
    wait_for_health(&client, public_base_url, None, 150).await?;
    verify_public_console(
        public_base_url,
        &installation.operator.identity_authorization_endpoint,
    )
    .await?;

    let jwks_url = format!("{}/oauth/jwks.json", public_base_url.trim_end_matches('/'));
    let jwks: Value = client
        .get(&jwks_url)
        .send()
        .await
        .context("requesting the public installation JWKS")?
        .error_for_status()
        .context("public installation JWKS returned an error")?
        .json()
        .await
        .context("decoding the public installation JWKS")?;
    ensure!(
        jwks.get("keys")
            .and_then(Value::as_array)
            .is_some_and(|keys| {
                keys.iter().any(|key| {
                    key.get("kid").and_then(Value::as_str)
                        == Some(installation.operator.access_token_key_id.as_str())
                })
            }),
        "public endpoint did not expose the installation authorization-server key"
    );
    verify_large_artifact_delivery(conformance, installation).await?;

    println!(
        "Installation verify ok: declared deployments and GPU capacity, public Console and authorization endpoints, configured signing key, and full/HEAD/range artifact delivery passed"
    );
    Ok(())
}

async fn verify_large_artifact_delivery(
    conformance: &Path,
    installation: &InstalledTarget,
) -> Result<()> {
    let base = installation.public_base();
    let profile = installation.profile();
    let token = installation.token().await?;

    let database = DuckDbDatabaseId::new("artifact_delivery_acceptance")?;
    let mut setup = DuckDbExecuteRequest::new(
        database.clone(),
        "CREATE OR REPLACE TABLE marker AS SELECT 1 AS ready".parse()?,
    );
    setup.create_if_missing = true;
    let setup_arguments = serde_json::to_string(&setup)?;
    let execute = run_public_conformance(
        conformance,
        base,
        profile,
        &token,
        &[
            "call",
            "--tool-name",
            "duckdb__execute",
            "--arguments",
            &setup_arguments,
        ],
        Duration::from_secs(60),
    )
    .await?;
    let execute: DuckDbExecuteOutput = serde_json::from_value(structured_output(&execute)?)
        .context("large-artifact setup returned invalid DuckDB output")?;
    ensure!(
        execute.db == database,
        "large-artifact setup returned an unexpected DuckDB identity"
    );

    let export_sql = format!(
        "SELECT i, sha256(CAST(i AS VARCHAR)) AS digest FROM range({LARGE_ARTIFACT_ROWS}) AS t(i) ORDER BY i"
    );
    let arguments = DuckDbExportRequest::Tabular {
        db: database.clone(),
        selection: DuckDbTabularSelection::Sql {
            sql: export_sql.parse()?,
        },
        format: DuckDbTabularFormat::Csv,
    };
    let export: DuckDbExportOutput = serde_json::from_value(
        export_with_task_notification(base, profile, &token, serde_json::to_value(arguments)?)
            .await?,
    )
    .context("large-artifact export returned invalid DuckDB output")?;
    ensure!(
        export.db() == &database && export.rows_exported() == LARGE_ARTIFACT_ROWS,
        "large-artifact export returned an unexpected database or row count"
    );
    let artifact = export.artifact();
    let artifact_id = artifact.artifact_id();
    ensure!(
        artifact.download_url.is_none(),
        "artifact metadata must not expose storage download plumbing"
    );

    let expected = expected_large_artifact();
    ensure!(
        expected.len() > LARGE_ARTIFACT_MINIMUM_BYTES,
        "large-artifact fixture must remain larger than 8 MiB"
    );
    ensure!(
        artifact.byte_len == expected.len() as u64,
        "artifact metadata byte length does not match deterministic export"
    );
    let expected_digest = Sha256::digest(&expected);
    let download_url = format!("{base}/artifacts/{profile}/{artifact_id}/download");
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

// Keep the former CLI's 210-second envelope and 180-second Task wait.
// The maintained Task helper registers uncertain dispatch cleanup before effects.
async fn export_with_task_notification(
    base: &str,
    profile: &str,
    token: &str,
    arguments: Value,
) -> Result<Value> {
    let overall = tokio::time::Instant::now() + Duration::from_secs(210);
    let mut endpoint = url::Url::parse(base).context("invalid installation MCP base")?;
    endpoint
        .path_segments_mut()
        .map_err(|_| anyhow!("installation MCP base cannot hold route segments"))?
        .pop_if_empty()
        .push("mcp")
        .push(profile);
    let client = tokio::time::timeout_at(
        overall.min(tokio::time::Instant::now() + Duration::from_secs(15)),
        connect_mcp_client(endpoint.as_str(), token),
    )
    .await
    .context("DuckDB SDK connection exceeded its admission deadline")?
    .map_err(|_| anyhow!("DuckDB SDK connection failed"))?;
    let mut subscription = None;
    let outcome = tokio::time::timeout_at(overall, async {
        let task = call_tool_as_task(&client, "duckdb__export", arguments)
            .await
            .map_err(|_| {
                anyhow!("DuckDB Task dispatch failed; original outcome remains unresolved")
            })?;
        let task_id = veoveo_types::CanonicalTaskId::parse(&task.task_id)
            .map_err(|_| anyhow!("DuckDB Task response has an invalid identity"))?;
        let deadline = overall.min(tokio::time::Instant::now() + Duration::from_secs(180));
        tokio::time::timeout_at(deadline, async {
            let filter = SubscriptionFilter::builder()
                .task_ids([task_id.to_string()])
                .build();
            subscription = Some(
                client
                    .listen(filter.clone())
                    .await
                    .map_err(|_| anyhow!("DuckDB exact Task listener failed"))?,
            );
            let stream = subscription.as_mut().expect("listener established");
            ensure!(
                stream.acknowledged() == &filter,
                "DuckDB Task listener changed its acknowledged exact filter"
            );
            loop {
                match stream
                    .next()
                    .await
                    .map_err(|_| anyhow!("DuckDB Task notification read failed"))?
                    .context("DuckDB Task listener ended before completion")?
                {
                    ServerNotification::TaskStatusNotification(update) => {
                        ensure!(
                            update.params.task.task.task_id == task_id.as_str(),
                            "DuckDB Task notification identity mismatch"
                        );
                        match update.params.task.status() {
                            TaskStatus::Completed => break,
                            TaskStatus::Working | TaskStatus::InputRequired => {}
                            _ => bail!("DuckDB export Task did not complete successfully"),
                        }
                    }
                    _ => bail!("DuckDB exact Task listener delivered an unexpected notification"),
                }
            }
            let current = client
                .get_task(GetTaskParams::new(task_id.to_string()))
                .await
                .map_err(|_| anyhow!("DuckDB current Task read failed"))?;
            ensure!(
                current.task.task.task_id == task_id.as_str()
                    && current.task.status() == TaskStatus::Completed,
                "DuckDB completed notification disagrees with current Task identity/status"
            );
            let payload = task_payload(&client, task_id.as_str())
                .await
                .map_err(|_| anyhow!("DuckDB completed Task payload read failed"))?;
            ensure!(
                payload.is_error != Some(true),
                "DuckDB export Task returned a tool error"
            );
            payload
                .structured_content
                .context("DuckDB export Task omitted structured content")
        })
        .await
        .context("DuckDB Task notification exceeded the original 180-second deadline")?
    })
    .await
    .context("DuckDB export exceeded the original 210-second deadline")
    .and_then(|result| result);
    // Cleanup runs after every dispatch/listener outcome, including a cancelled wait.
    let unsubscribe = if let Some(mut stream) = subscription {
        tokio::time::timeout(Duration::from_secs(5), stream.cancel())
            .await
            .context("DuckDB Task subscription cleanup exceeded five seconds")
            .and_then(|result| {
                result.map_err(|_| anyhow!("DuckDB Task subscription cleanup failed"))
            })
    } else {
        Ok(())
    };
    let close = tokio::time::timeout(Duration::from_secs(10), client.cancel())
        .await
        .context("DuckDB SDK cleanup exceeded ten seconds")
        .and_then(|result| result.map_err(|_| anyhow!("DuckDB SDK cleanup failed")));
    // Preserve the original failure; cleanup never turns unknown work into success.
    let value = outcome?;
    unsubscribe?;
    close?;
    Ok(value)
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

pub(super) async fn run_public_conformance(
    conformance: &Path,
    base: &str,
    profile: &str,
    token: &str,
    operation: &[&str],
    timeout: Duration,
) -> Result<String> {
    let url = format!("{base}/mcp/{profile}");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args(["--url", &url])
        .args(operation)
        .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")
        .env("MCP_BEARER_TOKEN", token)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(
        timeout,
        veoveo_testing_support::output_async(command, Duration::from_secs(60)),
    )
    .await
    .with_context(|| format!("public conformance operation {operation:?} timed out"))??;
    ensure!(
        output.status.success(),
        "public conformance operation {operation:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).context("decoding public conformance output")
}

pub(super) fn structured_output(output: &str) -> Result<Value> {
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

async fn verify_public_console(
    public_base_url: &str,
    authorization_endpoint: &url::Url,
) -> Result<()> {
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
        .context("requesting the public installation root")?;
    ensure!(
        root.status() == StatusCode::PERMANENT_REDIRECT
            && root
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                == Some("/console/"),
        "public installation root must redirect permanently to /console/"
    );

    let console_url = base.join("/console/")?;
    let console = browser
        .get(console_url)
        .send()
        .await
        .context("requesting the public installation console")?
        .error_for_status()
        .context("public installation console returned an error")?;
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
        authorization_endpoint_matches(&identity_provider, authorization_endpoint),
        "Console authorization must continue at the configured identity-provider endpoint"
    );
    Ok(())
}

fn assert_available_deployment(context: &str, namespace: &str, deployment: &str) -> Result<()> {
    let output = run_checked(
        Path::new("kubectl"),
        [
            "--request-timeout=30s",
            "--context",
            context,
            "--namespace",
            namespace,
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
            "--request-timeout=30s",
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
        "the installation requires at least {minimum} allocatable NVIDIA GPU shares; {context} reports {capacity}"
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

fn authorization_endpoint_matches(actual: &url::Url, expected: &url::Url) -> bool {
    actual.origin() == expected.origin()
        && actual.path() == expected.path()
        && actual.username().is_empty()
        && actual.password().is_none()
        && actual.fragment().is_none()
        && expected.query_pairs().all(|(key, _)| {
            let expected_values = expected
                .query_pairs()
                .filter(|(name, _)| name == &key)
                .map(|(_, value)| value.into_owned())
                .collect::<Vec<_>>();
            let actual_values = actual
                .query_pairs()
                .filter(|(name, _)| name == &key)
                .map(|(_, value)| value.into_owned())
                .collect::<Vec<_>>();
            actual_values == expected_values
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_redirect_must_match_configured_origin_path_and_static_parameters() {
        let expected =
            url::Url::parse("https://identity.example.test/tenant/authorize?policy=staff").unwrap();
        let actual = url::Url::parse("https://identity.example.test/tenant/authorize?policy=staff&state=opaque&client_id=console").unwrap();
        assert!(authorization_endpoint_matches(&actual, &expected));
        for wrong in [
            "https://other.example.test/tenant/authorize?policy=staff",
            "https://identity.example.test/other/authorize?policy=staff",
            "https://identity.example.test/tenant/authorize?policy=guests",
            "https://identity.example.test/tenant/authorize?policy=staff&policy=guests",
            "http://identity.example.test/tenant/authorize?policy=staff",
        ] {
            assert!(!authorization_endpoint_matches(
                &url::Url::parse(wrong).unwrap(),
                &expected
            ));
        }
    }
}
