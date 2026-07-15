use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    process::{Command, Stdio},
};

use anyhow::ensure;
use scraper::{Html, Selector};
use secrecy::{ExposeSecret, SecretString};

use super::*;

const NAMESPACE: &str = "veoveo";
const BIOMA_DEPLOYMENTS: &[&str] = &[
    "mcp-gateway",
    "artifact-service",
    "console-bff",
    "recording",
    "artifact-mcp",
    "media-mcp",
    "perception-mcp",
    "timeseries-mcp",
    "duckdb-mcp",
    "optimization-mcp",
    "frames-mcp",
    "map-mcp",
    "view-mcp",
    "time-mcp",
    "datasheet-mcp",
    "chart-mcp",
    "rerun-bridge",
    "cloudflared",
];

pub(crate) fn bioma_resources(context: &str) -> Result<()> {
    ensure!(!context.trim().is_empty(), "Kubernetes context is required");

    kubectl_apply(
        context,
        &serde_json::json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {"name": NAMESPACE}
        }),
    )?;

    let gateway = fs::read_to_string("examples/bioma/gateway.json")
        .context("reading the Bioma gateway control plane")?;
    kubectl_apply(
        context,
        &serde_json::json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {"name": "bioma-gateway-control-plane", "namespace": NAMESPACE},
            "data": {"gateway.json": gateway}
        }),
    )?;

    kubectl_apply(
        context,
        &secret_manifest(
            "veoveo-surreal-admin",
            [
                ("username", "VEOVEO_SURREAL_ADMIN_USERNAME"),
                ("password", "VEOVEO_SURREAL_ADMIN_PASSWORD"),
            ],
        )?,
    )?;
    let internal_trust_jwks = required_secret("VEOVEO_INTERNAL_TRUST_JWKS")?;
    GatewayInternalTrustBundle::from_json(internal_trust_jwks.expose_secret())
        .context("VEOVEO_INTERNAL_TRUST_JWKS must be canonical JSON")?;
    kubectl_apply(
        context,
        &secret_manifest(
            "veoveo-surreal-runtime",
            [
                ("username", "VEOVEO_SURREAL_RUNTIME_USERNAME"),
                ("password", "VEOVEO_SURREAL_RUNTIME_PASSWORD"),
            ],
        )?,
    )?;
    kubectl_apply(
        context,
        &secret_manifest(
            "veoveo-installation-secrets",
            [
                (
                    "internal-signing-key-der-b64",
                    "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                ),
                ("internal-signing-key-id", "VEOVEO_INTERNAL_SIGNING_KEY_ID"),
                ("internal-trust-jwks", "VEOVEO_INTERNAL_TRUST_JWKS"),
                ("oidc-client-secret", "VEOVEO_IDP_OIDC_CLIENT_SECRET"),
                (
                    "authorization-server-private-key-der-b64",
                    "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                ),
                (
                    "refresh-delivery-key-b64",
                    "VEOVEO_REFRESH_DELIVERY_KEY_B64",
                ),
                ("console-session-key", "VEOVEO_CONSOLE_SESSION_KEY"),
                ("object-store-access-key", "VEOVEO_OBJECT_STORE_ACCESS_KEY"),
                ("object-store-secret-key", "VEOVEO_OBJECT_STORE_SECRET_KEY"),
                ("media-provider-api-key", "MEDIA_PROVIDER_API_KEY"),
                (
                    "media-provider-webhook-secret",
                    "MEDIA_PROVIDER_WEBHOOK_SECRET",
                ),
            ],
        )?,
    )?;
    kubectl_apply(
        context,
        &secret_manifest("bioma-cloudflared", [("token", "CLOUDFLARED_TUNNEL_TOKEN")])?,
    )?;

    println!("Bioma Kubernetes resources applied to {context}");
    Ok(())
}

pub(crate) async fn bioma_verify(
    context: &str,
    local_base_url: &str,
    public_base_url: &str,
    object_base_url: &str,
) -> Result<()> {
    run_checked(
        Path::new("kubectl"),
        ["--context", context, "cluster-info"].map(OsString::from),
        [],
    )
    .with_context(|| format!("Kubernetes context {context} is unavailable"))?;

    for deployment in BIOMA_DEPLOYMENTS {
        assert_available_deployment(context, deployment)?;
    }
    assert_gpu_capacity(context, 2)?;

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

    let object = url::Url::parse(object_base_url).context("parsing public object-store URL")?;
    ensure!(
        object.scheme() == "https",
        "public object-store URL must use HTTPS"
    );
    let object_status = client
        .get(object)
        .send()
        .await
        .context("requesting the public Bioma object-store edge")?
        .status();
    ensure!(
        !object_status.is_server_error(),
        "public Bioma object-store edge returned {object_status}"
    );

    println!(
        "Bioma verify ok: the full server catalog is available, both GPU workloads are schedulable, console assets and Entra authorization are public, object TLS is valid, and the Bioma JWKS is authoritative"
    );
    Ok(())
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

fn secret_manifest<const N: usize>(name: &str, mappings: [(&str, &str); N]) -> Result<Value> {
    let mut string_data = BTreeMap::new();
    for (key, environment_name) in mappings {
        let value = required_secret(environment_name)?;
        string_data.insert(key, value.expose_secret().to_owned());
    }
    Ok(serde_json::json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": name, "namespace": NAMESPACE},
        "type": "Opaque",
        "stringData": string_data
    }))
}

fn required_secret(name: &str) -> Result<SecretString> {
    let value = env::var(name).with_context(|| format!("required environment variable {name}"))?;
    ensure!(
        !value.trim().is_empty(),
        "required environment variable {name} is empty"
    );
    Ok(SecretString::from(value))
}

fn kubectl_apply(context: &str, manifest: &Value) -> Result<()> {
    let mut child = Command::new("kubectl")
        .args(["--context", context, "apply", "-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning kubectl apply")?;
    let encoded = serde_json::to_vec(manifest)?;
    child
        .stdin
        .take()
        .context("kubectl stdin was unavailable")?
        .write_all(&encoded)
        .context("writing Kubernetes resource to kubectl")?;
    let output = child
        .wait_with_output()
        .context("waiting for kubectl apply")?;
    if !output.status.success() {
        bail!(
            "kubectl apply failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
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
        "Bioma requires at least {minimum} allocatable NVIDIA GPU shares; {context} reports {capacity}"
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
