use super::*;

pub(crate) async fn gateway_http(
    conformance: &Path,
    gateway: &Path,
    base_control_plane: &Path,
) -> Result<()> {
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
    let control_db = spawn_gateway_control_db(gateway, &control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(port, &control_db.url, &state_db),
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
    assert_ready_profiles(&base, 2).await?;
    let untrusted_host_status = reqwest::Client::new()
        .get(format!(
            "{base}/.well-known/oauth-protected-resource/mcp/operator"
        ))
        .header(HOST, "evil.example.com")
        .send()
        .await?
        .status();
    if untrusted_host_status != StatusCode::MISDIRECTED_REQUEST {
        bail!("untrusted Host status was {untrusted_host_status}, expected 421");
    }
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
            format!("{base}/mcp/operator").into(),
            "auth-discovery".into(),
            "--metadata-url".into(),
            format!("{base}/.well-known/oauth-protected-resource/mcp/operator").into(),
            "--authorization-server-metadata-url".into(),
            format!("{base}/.well-known/oauth-authorization-server/oauth").into(),
            "--authorization-server-jwks-url".into(),
            format!("{base}/oauth/jwks.json").into(),
            "--required-scope".into(),
            "operator:use".into(),
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
            "operator:use",
            "--jwt-id",
            client_assertion_replay_jti,
        ],
    )?;
    if gateway_token(
        conformance,
        &base,
        &[
            "--scope",
            "operator:use",
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
    let local_client_id = "operator-local-public";
    let local_redirect_uri = "http://127.0.0.1:8789/oauth/callback";
    let hosted_client_id = "operator-hosted-public";
    let hosted_redirect_uri = "https://chatgpt.com/connector_platform_oauth_redirect";
    let code_verifier = "smoke-browser-pkce-verifier-0123456789abcdef0123456789abcdef";
    let code_challenge = "X9AgXux1PHu8RKlqHF9FuDYoLL6yjPFGS5je8BbaBF8";
    let (gateway_code, callback_query) = gateway_browser_authorization_code(
        &http,
        &idp_client,
        &base,
        &idp_base,
        local_client_id,
        local_redirect_uri,
        code_challenge,
        "smoke-state",
    )
    .await?;

    let token_response: Value = http
        .post(format!("{base}/oauth/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", local_client_id),
            ("code", gateway_code.as_str()),
            ("redirect_uri", local_redirect_uri),
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
        .post(format!("{base}/oauth/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", local_client_id),
            ("code", gateway_code.as_str()),
            ("redirect_uri", local_redirect_uri),
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
        local_client_id,
        local_redirect_uri,
        code_challenge,
        "smoke-wrong-pkce",
    )
    .await?;
    let wrong_pkce_status = http
        .post(format!("{base}/oauth/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", local_client_id),
            ("code", wrong_pkce_code.as_str()),
            ("redirect_uri", local_redirect_uri),
            ("code_verifier", wrong_code_verifier),
        ]))
        .send()
        .await?
        .status();
    if wrong_pkce_status != StatusCode::BAD_REQUEST {
        bail!("wrong PKCE verifier status was {wrong_pkce_status}, expected 400");
    }
    let wrong_pkce_redeem_status = http
        .post(format!("{base}/oauth/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", local_client_id),
            ("code", wrong_pkce_code.as_str()),
            ("redirect_uri", local_redirect_uri),
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
        &format!("{base}/oauth/callback?{callback_query}"),
        StatusCode::BAD_REQUEST,
    )
    .await?;

    let (hosted_gateway_code, _) = gateway_browser_authorization_code(
        &http,
        &idp_client,
        &base,
        &idp_base,
        hosted_client_id,
        hosted_redirect_uri,
        code_challenge,
        "smoke-hosted-state",
    )
    .await?;
    let hosted_token_response: Value = http
        .post(format!("{base}/oauth/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", hosted_client_id),
            ("code", hosted_gateway_code.as_str()),
            ("redirect_uri", hosted_redirect_uri),
            ("code_verifier", code_verifier),
        ]))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if hosted_token_response
        .get("token_type")
        .and_then(Value::as_str)
        != Some("Bearer")
    {
        bail!("hosted authorization-code token response was not bearer: {hosted_token_response}");
    }

    let admin_token = gateway_token_for_profile(
        conformance,
        &base,
        "admin",
        &["--scope", "operator:use", "--scope", "admin:manage"],
    )?;
    let revocation = post_json(
        &http,
        &format!("{base}/admin/admin/jwt-revocations"),
        Some(admin_token.trim()),
        serde_json::json!({
            "profile": "operator",
            "issuer": "https://veoveo.bioma.ai/oauth",
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
        &format!("{base}/admin/admin/jwt-revocations/prune"),
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
        .post(format!("{base}/admin/admin/jwt-revocations"))
        .bearer_auth(admin_token.trim())
        .json(&serde_json::json!({
            "profile": "operator",
            "issuer": "https://veoveo.bioma.ai/oauth",
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
    let auth_method_summary = run_gateway_json(gateway, "auth-audit-method-summary", &state_db)?;
    assert_audit_method(&auth_method_summary, "bearer_jwt", 2, 1)?;
    assert_audit_method(
        &auth_method_summary,
        "client_credentials_private_key_jwt",
        2,
        1,
    )?;
    assert_audit_method(&auth_method_summary, "oidc_authorization_code_pkce", 1, 2)?;
    let auth_reason_summary = run_gateway_json(gateway, "auth-audit-reason-summary", &state_db)?;
    assert_reason_summary_at_least(&auth_reason_summary, "auth_allow", 4)?;
    assert_reason_summary_at_least(&auth_reason_summary, "missing_authorization_header", 1)?;
    assert_reason_summary_at_least(&auth_reason_summary, "client_assertion_replay", 1)?;
    assert_reason_summary_at_least(&auth_reason_summary, "invalid_authorization_code", 1)?;
    assert_reason_summary_at_least(&auth_reason_summary, "invalid_pkce", 1)?;
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
