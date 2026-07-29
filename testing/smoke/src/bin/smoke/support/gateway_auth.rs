use super::*;
use anyhow::ensure;

pub(crate) fn gateway_id_jag_token(
    conformance: &Path,
    gateway_base: &str,
    args: &[&str],
) -> Result<String> {
    gateway_id_jag_token_for_profile(conformance, gateway_base, "operator", args)
}

pub(crate) fn gateway_hosted_public_id_jag_token(
    conformance: &Path,
    gateway_base: &str,
    args: &[&str],
) -> Result<String> {
    gateway_id_jag_token_for_client(
        conformance,
        gateway_base,
        "operator",
        "operator-hosted-delegated",
        args,
    )
}

pub(crate) fn gateway_id_jag_token_for_profile(
    conformance: &Path,
    gateway_base: &str,
    profile: &str,
    args: &[&str],
) -> Result<String> {
    let client_id = if profile == "admin" {
        "admin-delegated"
    } else {
        "operator-delegated"
    };
    gateway_id_jag_token_for_client(conformance, gateway_base, profile, client_id, args)
}

fn gateway_id_jag_token_for_client(
    conformance: &Path,
    gateway_base: &str,
    profile: &str,
    client_id: &str,
    args: &[&str],
) -> Result<String> {
    let mut all_args = vec![
        "gateway-id-jag-token-exchange".into(),
        "--token-url".into(),
        format!("{gateway_base}/oauth/token").into(),
        "--audience".into(),
        format!("{PUBLIC_BASE_URL}/oauth").into(),
        "--resource".into(),
        format!("{PUBLIC_BASE_URL}/mcp/{profile}").into(),
        "--client-id".into(),
        client_id.into(),
    ];
    all_args.extend(args.iter().map(|arg| OsString::from(*arg)));
    run_checked(conformance, all_args, [])
}

pub(crate) fn gateway_token(
    conformance: &Path,
    gateway_base: &str,
    args: &[&str],
) -> Result<String> {
    gateway_token_for_profile(conformance, gateway_base, "operator", args)
}

pub(crate) fn gateway_token_for_profile(
    conformance: &Path,
    gateway_base: &str,
    profile: &str,
    args: &[&str],
) -> Result<String> {
    let client_id = if profile == "admin" {
        "admin-service"
    } else {
        "operator-service"
    };
    let mut all_args = vec![
        "gateway-token-exchange".into(),
        "--token-url".into(),
        format!("{gateway_base}/oauth/token").into(),
        "--client-id".into(),
        client_id.into(),
        "--audience".into(),
        format!("{PUBLIC_BASE_URL}/oauth/token").into(),
        "--resource".into(),
        format!("{PUBLIC_BASE_URL}/mcp/{profile}").into(),
    ];
    all_args.extend(args.iter().map(|arg| OsString::from(*arg)));
    run_checked(conformance, all_args, [])
}

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

pub(crate) fn run_gateway_json(
    gateway: &Path,
    command: &str,
    platform: &PlatformStoreSmoke,
) -> Result<Value> {
    let output = run_checked(gateway, [command.into()], platform.runtime_env())?;
    Ok(serde_json::from_str(&output)?)
}

pub(crate) fn run_gateway_metadata_summary(
    gateway: &Path,
    platform: &PlatformStoreSmoke,
    metadata_key: &str,
) -> Result<Value> {
    let output = run_checked(
        gateway,
        [
            "audit-metadata-summary".into(),
            "--metadata-key".into(),
            metadata_key.into(),
        ],
        platform.runtime_env(),
    )?;
    Ok(serde_json::from_str(&output)?)
}

pub(crate) fn run_gateway_auth_metadata_summary(
    gateway: &Path,
    platform: &PlatformStoreSmoke,
    metadata_key: &str,
) -> Result<Value> {
    let output = run_checked(
        gateway,
        [
            "auth-audit-metadata-summary".into(),
            "--metadata-key".into(),
            metadata_key.into(),
        ],
        platform.runtime_env(),
    )?;
    Ok(serde_json::from_str(&output)?)
}
