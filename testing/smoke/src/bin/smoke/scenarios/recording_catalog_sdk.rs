//! Installed native Redap query through the external Rerun Catalog SDK.

use super::*;
use anyhow::{Context, ensure};
use tokio::io::AsyncWriteExt;

const UV_IMAGE: &str = "ghcr.io/astral-sh/uv:python3.12-bookworm@sha256:85d4cb1afa769a7338e095b927bee941cf5ec92266c7424b3f6c0f2748567248";

pub(crate) async fn recording_catalog_sdk(
    conformance: &Path,
    installation: &InstalledTarget,
    dataset_id: uuid::Uuid,
    recording_id: veoveo_recording_contract::RecordingId,
) -> Result<()> {
    let target = &installation.target;
    let catalog = target
        .recording_catalog
        .as_ref()
        .context("recording-catalog-sdk requires recordingCatalog in the installation target")?;
    run_checked(
        Path::new("kubectl"),
        [
            "--request-timeout=30s",
            "--context",
            &target.kubernetes.context,
            "cluster-info",
        ]
        .map(OsString::from),
        [],
    )?;
    let token = installation.token(conformance).await?;
    let base = installation.public_base();
    let profile = installation.profile();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let grant_url = format!("{base}/recordings/{profile}/catalog-grants");
    let grant_request = serde_json::json!({
        "dataset_id": dataset_id,
        "recording_ids": [recording_id],
    });
    let first: serde_json::Value = client
        .post(&grant_url)
        .bearer_auth(&token)
        .json(&grant_request)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let renewed: serde_json::Value = client
        .post(&grant_url)
        .bearer_auth(&token)
        .json(&grant_request)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let first_redap_token = first["redap_token"]
        .as_str()
        .context("first catalog grant has no Redap token")?;
    let renewed_redap_token = renewed["redap_token"]
        .as_str()
        .context("renewed catalog grant has no Redap token")?;
    let grants = serde_json::to_vec(&serde_json::json!({"first": first, "renewed": renewed}))?;
    let ingress_host = target
        .local_base_url
        .host()
        .context("local ingress host is missing")?
        .to_string();
    let ingress_host = ingress_host.trim_matches(['[', ']']);
    let ingress = (
        ingress_host,
        target
            .local_base_url
            .port_or_known_default()
            .context("local ingress port is missing")?,
    );
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if tokio::net::TcpStream::connect(ingress).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .context("installation loopback ingress did not become ready")?;

    let cwd = std::env::current_dir()?;
    let mount = format!("{0}:{0}:ro", cwd.display());
    let container_name = format!("veoveo-catalog-sdk-{}", uuid::Uuid::now_v7().simple());
    let _container = ContainerGuard::new(&container_name);
    let mut command = tokio::process::Command::new("docker");
    command.args(["run", "--name", &container_name]);
    for mapping in &catalog.host_mappings {
        let address = match mapping.address {
            std::net::IpAddr::V4(ip) => ip.to_string(),
            std::net::IpAddr::V6(ip) => format!("[{ip}]"),
        };
        command.args(["--add-host", &format!("{}:{address}", mapping.hostname)]);
    }
    command
        .args([
            "--rm",
            "--interactive",
            "--network=host",
            "--volume",
            &mount,
            "--volume=veoveo-catalog-uv-cache:/tmp/uv-cache",
            "--volume=veoveo-catalog-venv:/tmp/veoveo-catalog-venv",
            "--workdir",
            cwd.to_str().context("repository path is not UTF-8")?,
            "--env=UV_PROJECT_ENVIRONMENT=/tmp/veoveo-catalog-venv",
            "--env=UV_CACHE_DIR=/tmp/uv-cache",
            "--env=UV_LINK_MODE=copy",
            UV_IMAGE,
            "uv",
            "run",
            "--project",
            "testing/recording-catalog-sdk",
            "--locked",
            "python",
            "testing/recording-catalog-sdk/smoke.py",
            "--redap-url",
            catalog.redap_url.as_str(),
            "--dataset-id",
            &dataset_id.to_string(),
            "--recording-id",
            &recording_id.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("starting pinned Rerun Catalog SDK container")?;
    child
        .stdin
        .take()
        .context("Catalog SDK process has no stdin")?
        .write_all(&grants)
        .await?;
    let output = tokio::time::timeout(Duration::from_secs(240), child.wait_with_output())
        .await
        .context("Rerun Catalog SDK query timed out")??;
    ensure!(
        output.status.success(),
        "Rerun Catalog SDK query failed: {}",
        String::from_utf8_lossy(&output.stderr)
            .replace(&token, "[redacted]")
            .replace(first_redap_token, "[redacted]")
            .replace(renewed_redap_token, "[redacted]")
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    ensure!(
        result.get("renewed").and_then(serde_json::Value::as_bool) == Some(true)
            && result
                .get("rows")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                > 0,
        "Rerun Catalog SDK did not prove rows and renewed access"
    );
    println!("Native Rerun Catalog SDK query and grant renewal passed: {result}");
    Ok(())
}
