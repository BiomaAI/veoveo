use super::*;

pub fn gateway_id_jag_token(gateway_base: &str, args: &[&str]) -> Result<String> {
    gateway_id_jag_token_for_profile(gateway_base, "operator", args)
}

pub fn gateway_hosted_public_id_jag_token(gateway_base: &str, args: &[&str]) -> Result<String> {
    gateway_id_jag_token_for_client(gateway_base, "operator", "operator-hosted-delegated", args)
}

pub fn gateway_id_jag_token_for_profile(
    gateway_base: &str,
    profile: &str,
    args: &[&str],
) -> Result<String> {
    let client_id = if profile == "admin" {
        "admin-delegated"
    } else {
        "operator-delegated"
    };
    gateway_id_jag_token_for_client(gateway_base, profile, client_id, args)
}

fn gateway_id_jag_token_for_client(
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
    run_checked(
        &veoveo_testing_support::artifacts::executable(
            "veoveo-gateway-composition",
            "gateway-smoke-support",
        )?,
        all_args,
        [],
    )
}

pub fn gateway_token(gateway_base: &str, args: &[&str]) -> Result<String> {
    gateway_token_for_profile(gateway_base, "operator", args)
}

pub fn gateway_token_for_profile(
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
    run_checked(
        &veoveo_testing_support::artifacts::executable(
            "veoveo-gateway-composition",
            "gateway-smoke-support",
        )?,
        all_args,
        [],
    )
}
