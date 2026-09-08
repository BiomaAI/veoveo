use anyhow::{Context, Result, ensure};
use std::{path::Path, process::Stdio, time::Duration};

pub(crate) async fn gateway_token_for_context(
    conformance: &Path,
    base: &str,
    client_id: &str,
    profile: &str,
    scopes: &[&str],
    work_context: &str,
) -> Result<String> {
    let token_url = format!("{base}/oauth/token");
    let resource = format!("{base}/mcp/{profile}");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args([
            "gateway-token-exchange",
            "--token-url",
            &token_url,
            "--client-id",
            client_id,
            "--audience",
            &token_url,
            "--resource",
            &resource,
            "--work-context",
            work_context,
        ])
        .args(
            scopes
                .iter()
                .flat_map(|scope| ["--scope", *scope])
                .collect::<Vec<_>>(),
        )
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .context("gateway token exchange timed out")??;
    ensure!(
        output.status.success(),
        "gateway token exchange failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let token = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(!token.is_empty(), "gateway returned an empty access token");
    Ok(token)
}
