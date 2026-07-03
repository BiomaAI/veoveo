use super::*;

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

pub(crate) fn run_gateway_auth_metadata_summary(
    gateway: &Path,
    state_db: &Path,
    metadata_key: &str,
) -> Result<Value> {
    let output = run_checked(
        gateway,
        [
            "auth-audit-metadata-summary".into(),
            "--state-db".into(),
            state_db.as_os_str().to_os_string(),
            "--metadata-key".into(),
            metadata_key.into(),
        ],
        [],
    )?;
    Ok(serde_json::from_str(&output)?)
}
