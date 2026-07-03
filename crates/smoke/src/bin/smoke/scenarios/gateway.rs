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
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(port, &control_plane, &state_db),
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
    assert_ready_profiles(&base, 1).await?;
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
            format!("{base}/mcp/default").into(),
            "auth-discovery".into(),
            "--metadata-url".into(),
            format!("{base}/.well-known/oauth-protected-resource/mcp/default").into(),
            "--authorization-server-metadata-url".into(),
            format!("{base}/.well-known/oauth-authorization-server/oauth/default").into(),
            "--authorization-server-jwks-url".into(),
            format!("{base}/oauth/default/jwks.json").into(),
            "--required-scope".into(),
            "media:use".into(),
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
            "media:use",
            "--jwt-id",
            client_assertion_replay_jti,
        ],
    )?;
    if gateway_token(
        conformance,
        &base,
        &[
            "--scope",
            "media:use",
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
    let code_verifier = "smoke-browser-pkce-verifier-0123456789abcdef0123456789abcdef";
    let code_challenge = "X9AgXux1PHu8RKlqHF9FuDYoLL6yjPFGS5je8BbaBF8";
    let (gateway_code, callback_query) = gateway_browser_authorization_code(
        &http,
        &idp_client,
        &base,
        &idp_base,
        code_challenge,
        "smoke-state",
    )
    .await?;

    let token_response: Value = http
        .post(format!("{base}/oauth/default/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", "veoveo-browser"),
            ("code", gateway_code.as_str()),
            ("redirect_uri", "https://veoveo.bioma.ai/oauth/callback"),
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
        .post(format!("{base}/oauth/default/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", "veoveo-browser"),
            ("code", gateway_code.as_str()),
            ("redirect_uri", "https://veoveo.bioma.ai/oauth/callback"),
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
        code_challenge,
        "smoke-wrong-pkce",
    )
    .await?;
    let wrong_pkce_status = http
        .post(format!("{base}/oauth/default/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", "veoveo-browser"),
            ("code", wrong_pkce_code.as_str()),
            ("redirect_uri", "https://veoveo.bioma.ai/oauth/callback"),
            ("code_verifier", wrong_code_verifier),
        ]))
        .send()
        .await?
        .status();
    if wrong_pkce_status != StatusCode::BAD_REQUEST {
        bail!("wrong PKCE verifier status was {wrong_pkce_status}, expected 400");
    }
    let wrong_pkce_redeem_status = http
        .post(format!("{base}/oauth/default/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_urlencoded(&[
            ("grant_type", "authorization_code"),
            ("client_id", "veoveo-browser"),
            ("code", wrong_pkce_code.as_str()),
            ("redirect_uri", "https://veoveo.bioma.ai/oauth/callback"),
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
        &format!("{base}/oauth/default/callback?{callback_query}"),
        StatusCode::BAD_REQUEST,
    )
    .await?;
    assert_http_post_status(
        &format!("{base}/admin/default/reload-control-plane"),
        None,
        StatusCode::UNAUTHORIZED,
    )
    .await?;

    let admin_token = gateway_token(
        conformance,
        &base,
        &["--scope", "media:use", "--scope", "gateway:admin"],
    )?;
    let revocation = post_json(
        &http,
        &format!("{base}/admin/default/jwt-revocations"),
        Some(admin_token.trim()),
        serde_json::json!({
            "issuer": "https://veoveo.bioma.ai/oauth/default",
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
        &format!("{base}/admin/default/jwt-revocations/prune"),
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
        .post(format!("{base}/admin/default/jwt-revocations"))
        .bearer_auth(admin_token.trim())
        .json(&serde_json::json!({
            "issuer": "https://veoveo.bioma.ai/oauth/default",
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

pub(crate) async fn gateway_authenticated(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let media_port = 18801u16;
    let gateway_port = 18802u16;
    let edge_port = 18809u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let edge_base = format!("http://127.0.0.1:{edge_port}");
    let media_log = tmpdir.join("media.log");
    let gateway_log = tmpdir.join("gateway.log");
    let edge_log = tmpdir.join("edge.log");
    let edge_caddyfile = tmpdir.join("Caddyfile");
    let media_state_db = tmpdir.join("media-state.duckdb");
    let gateway_state_db = tmpdir.join("gateway-state.duckdb");

    let mut media_child = spawn_media_s3_smoke(
        media,
        media_port,
        PUBLIC_BASE_URL,
        &media_state_db,
        &media_log,
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;
    let health = reqwest::get(format!("{media_base}/media/healthz"))
        .await?
        .error_for_status()?
        .text()
        .await?;
    contains(&health, "ok")?;
    assert_json_log(
        &media_log,
        &[
            ("message", "listening"),
            ("service", "veoveo-media-mcp"),
            ("mcp_path", "/media/mcp"),
        ],
    )?;
    assert_json_log(&media_log, &[("message", "media retention gc completed")])?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, control_plane, &gateway_state_db),
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
    assert_ready_profiles(&gateway_base, 1).await?;
    assert_json_log(
        &gateway_log,
        &[("message", "listening"), ("service", "veoveo-mcp-gateway")],
    )?;
    assert_json_log(
        &gateway_log,
        &[("message", "gateway retention gc completed")],
    )?;

    write_edge_caddyfile(&edge_caddyfile, gateway_port, media_port)?;
    let edge_name = format!("veoveo-edge-smoke-{edge_port}-{}", uuid::Uuid::new_v4());
    let _edge_container = ContainerGuard::new(edge_name.clone());
    let mut edge = ChildGuard::spawn(
        Path::new("docker"),
        [
            "run".into(),
            "--rm".into(),
            "--name".into(),
            edge_name.into(),
            "--add-host=host.docker.internal:host-gateway".into(),
            "-p".into(),
            format!("127.0.0.1:{edge_port}:8080").into(),
            "-v".into(),
            format!("{}:/etc/caddy/Caddyfile:ro", edge_caddyfile.display()).into(),
            "caddy:2.11.2".into(),
            "caddy".into(),
            "run".into(),
            "--config".into(),
            "/etc/caddy/Caddyfile".into(),
            "--adapter".into(),
            "caddyfile".into(),
        ],
        [],
        &edge_log,
    )?;
    wait_for_http(&format!("{edge_base}/healthz")).await?;
    contains(
        &reqwest::get(format!("{edge_base}/healthz"))
            .await?
            .error_for_status()?
            .text()
            .await?,
        "ok",
    )?;
    contains(
        &reqwest::get(format!("{edge_base}/media/healthz"))
            .await?
            .error_for_status()?
            .text()
            .await?,
        "ok",
    )?;
    assert_http_status(&format!("{edge_base}/media/mcp"), StatusCode::NOT_FOUND).await?;
    let edge_token = gateway_token(conformance, &edge_base, &["--scope", "media:use"])?;
    run_direct_mcp(
        conformance,
        &format!("{edge_base}/mcp/default"),
        ["info".into()],
        [("MCP_BEARER_TOKEN", edge_token.trim().into())],
    )?;

    let admin_token = gateway_token(
        conformance,
        &gateway_base,
        &["--scope", "media:use", "--scope", "gateway:admin"],
    )?;
    let http = reqwest::Client::new();
    let reload = post_json(
        &http,
        &format!("{gateway_base}/admin/default/reload-control-plane"),
        Some(admin_token.trim()),
        Value::Null,
    )
    .await?;
    let reload_revision_id = assert_control_plane_admin_result(&reload, "reloaded")?;
    if !reload_revision_id.starts_with("gcp-")
        || reload.get("sha256").and_then(Value::as_str).map(str::len) != Some(64)
    {
        bail!("unexpected reload result: {reload}");
    }
    let control_status = get_json(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
    )
    .await?;
    assert_control_plane_status(&control_status, &reload_revision_id)?;

    let applied = put_json_file(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
        control_plane,
    )
    .await?;
    let revision_id = assert_control_plane_admin_result(&applied, "applied")?;
    let control_status = get_json(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
    )
    .await?;
    assert_control_plane_status(&control_status, &revision_id)?;

    let ops_control_plane = tmpdir.join("gateway.ops.json");
    write_ops_profile_control_plane(control_plane, &ops_control_plane)?;
    let ops_applied = put_json_file(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
        &ops_control_plane,
    )
    .await?;
    let ops_revision_id =
        assert_control_plane_admin_result_with_profiles(&ops_applied, "applied", 2)?;
    assert_ready_profiles(&gateway_base, 2).await?;
    let ops_admin_token = gateway_id_jag_token_for_profile(
        conformance,
        &gateway_base,
        "ops",
        &[
            "--id-jag-scope",
            "media:use",
            "--id-jag-scope",
            "gateway:admin",
            "--scope",
            "media:use",
            "--scope",
            "gateway:admin",
        ],
    )?;
    let ops_status = get_json(
        &http,
        &format!("{gateway_base}/admin/ops/control-plane"),
        Some(ops_admin_token.trim()),
    )
    .await?;
    assert_control_plane_status_with_profiles(&ops_status, &ops_revision_id, 2)?;
    let ops_token = gateway_id_jag_token_for_profile(
        conformance,
        &gateway_base,
        "ops",
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
            "--principal-assurance",
            "us_person",
        ],
    )?;
    run_direct_mcp(
        conformance,
        &format!("{gateway_base}/mcp/ops"),
        ["info".into()],
        [("MCP_BEARER_TOKEN", ops_token.trim().into())],
    )?;

    let reverted = put_json_file(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
        control_plane,
    )
    .await?;
    let reverted_revision_id = assert_control_plane_admin_result(&reverted, "applied")?;
    assert_ready_profiles(&gateway_base, 1).await?;
    assert_mcp_denied(
        conformance,
        &format!("{gateway_base}/mcp/ops"),
        ops_token.trim(),
        ["info".into()],
    )?;

    gateway_child.stop();
    gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, control_plane, &gateway_state_db),
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
    assert_ready_profiles(&gateway_base, 1).await?;
    let control_status = get_json(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
    )
    .await?;
    assert_control_plane_status(&control_status, &reverted_revision_id)?;

    let token = gateway_token(conformance, &gateway_base, &["--scope", "media:use"])?;
    let token = token.trim();
    let gateway_mcp_url = format!("{gateway_base}/mcp/default");
    for args in [
        vec!["info".into()],
        vec!["resources".into()],
        vec!["resource".into(), "media://usage".into()],
        vec!["prompts".into()],
        vec![
            "prompt".into(),
            "media-model-select".into(),
            "--arguments".into(),
            r#"{"goal":"choose an image generation model for a product render","media_type":"image","budget":"low"}"#.into(),
        ],
        vec!["tasks".into()],
    ] {
        run_direct_mcp(
            conformance,
            &gateway_mcp_url,
            args,
            [("MCP_BEARER_TOKEN", token.into())],
        )?;
    }

    let revoked_token = gateway_token(conformance, &gateway_base, &["--scope", "media:use"])?;
    let revoked_jti = jwt_id(revoked_token.trim())?;
    let revocation = post_json(
        &http,
        &format!("{gateway_base}/admin/default/jwt-revocations"),
        Some(admin_token.trim()),
        serde_json::json!({
            "issuer": "https://veoveo.bioma.ai/oauth/default",
            "jwt_id": revoked_jti,
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
            != Some(revoked_jti.as_str())
    {
        bail!("unexpected JWT revocation result: {revocation}");
    }
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        revoked_token.trim(),
        ["info".into()],
    )?;

    let ema_token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
            "--principal-assurance",
            "us_person",
        ],
    )?;
    let ema_token = ema_token.trim();
    run_direct_mcp(
        conformance,
        &gateway_mcp_url,
        ["info".into()],
        [("MCP_BEARER_TOKEN", ema_token.into())],
    )?;
    let live_policy_session = connect_mcp_session(&gateway_mcp_url, token).await?;
    read_mcp_resource_json(&live_policy_session, "media://usage").await?;

    let cui_control_plane = tmpdir.join("gateway.cui.json");
    write_cui_control_plane(control_plane, &cui_control_plane)?;
    let cui_apply = put_json_file(
        &http,
        &format!("{gateway_base}/admin/default/control-plane"),
        Some(admin_token.trim()),
        &cui_control_plane,
    )
    .await?;
    assert_control_plane_admin_result(&cui_apply, "applied")?;

    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        token,
        ["resource".into(), "media://usage".into()],
    )?;
    assert_mcp_session_resource_denied(&live_policy_session, "media://usage").await?;
    live_policy_session.cancel().await?;
    let missing_group_token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--role",
            "operator",
            "--data-label",
            "cui",
            "--principal-assurance",
            "us_person",
        ],
    )?;
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        missing_group_token.trim(),
        ["resource".into(), "media://usage".into()],
    )?;
    let missing_role_token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--data-label",
            "cui",
            "--principal-assurance",
            "us_person",
        ],
    )?;
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        missing_role_token.trim(),
        ["resource".into(), "media://usage".into()],
    )?;
    let missing_assurance_token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        missing_assurance_token.trim(),
        ["resource".into(), "media://usage".into()],
    )?;
    run_direct_mcp(
        conformance,
        &gateway_mcp_url,
        ["resource".into(), "media://usage".into()],
        [("MCP_BEARER_TOKEN", ema_token.into())],
    )?;

    let replay_jti = "smoke-id-jag-replay";
    gateway_id_jag_token(
        conformance,
        &gateway_base,
        &["--id-jag-scope", "media:use", "--jwt-id", replay_jti],
    )?;
    if gateway_id_jag_token(
        conformance,
        &gateway_base,
        &["--id-jag-scope", "media:use", "--jwt-id", replay_jti],
    )
    .is_ok()
    {
        bail!("replayed ID-JAG was unexpectedly accepted");
    }

    let denied_token = gateway_token(conformance, &gateway_base, &["--scope", "gateway:admin"])?;
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        denied_token.trim(),
        ["info".into()],
    )?;

    edge.stop();
    gateway_child.stop();
    let audit_counts = run_gateway_json(gateway, "audit-counts", &gateway_state_db)?;
    assert_json_u64_at_least(&audit_counts, "auth_events", 1)?;
    assert_json_u64_at_least(&audit_counts, "policy_events", 1)?;
    let audit_summary = run_gateway_json(gateway, "audit-method-summary", &gateway_state_db)?;
    assert_audit_method(&audit_summary, "tools/list", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/list", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/templates/list", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/read", 3, 5)?;
    assert_audit_method(&audit_summary, "prompts/list", 2, 0)?;
    assert_audit_method(&audit_summary, "prompts/get", 1, 0)?;
    assert_audit_method(&audit_summary, "tasks/list", 1, 0)?;
    let audit_reasons = run_gateway_json(gateway, "audit-reason-summary", &gateway_state_db)?;
    assert_reason_summary_at_least(&audit_reasons, "missing_data_label", 1)?;
    assert_reason_summary_at_least(&audit_reasons, "missing_principal_assurance", 1)?;
    assert_reason_summary_at_least(&audit_reasons, "missing_group", 1)?;
    assert_reason_summary_at_least(&audit_reasons, "missing_role", 1)?;
    let principal_kind_summary =
        run_gateway_metadata_summary(gateway, &gateway_state_db, "principal_kind")?;
    assert_metadata_summary_at_least(&principal_kind_summary, "user", 1)?;
    let principal_label_summary =
        run_gateway_metadata_summary(gateway, &gateway_state_db, "principal_data_labels")?;
    assert_metadata_summary_at_least(&principal_label_summary, "cui", 1)?;
    let principal_assurance_summary =
        run_gateway_metadata_summary(gateway, &gateway_state_db, "principal_assurances")?;
    assert_metadata_summary_at_least(&principal_assurance_summary, "us_person", 1)?;

    media_child.stop();
    cleanup.remove_on_drop();
    println!("gateway authenticated smoke ok");
    Ok(())
}

pub(crate) async fn gateway_two_servers(
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

pub(crate) async fn gateway_task_run(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let media_port = 18801u16;
    let gateway_port = 18802u16;
    let provider_port = 18806u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let provider_base = format!("http://127.0.0.1:{provider_port}");
    let provider_log = tmpdir.join("provider.log");
    let media_log = tmpdir.join("media.log");
    let gateway_log = tmpdir.join("gateway.log");
    let provider_ready = tmpdir.join("provider.ready");
    let media_state_db = tmpdir.join("media-state.duckdb");
    let gateway_state_db = tmpdir.join("gateway-state.duckdb");
    let output_dir = tmpdir.join("outputs");

    let mut provider = spawn_fake_media_provider(
        conformance,
        provider_port,
        &provider_ready,
        &provider_log,
        Some(4000),
    )?;
    wait_for_file_and_http(&provider_ready, &format!("{provider_base}/api/v3/models")).await?;

    let mut media_child = spawn_media_memory_smoke(
        media,
        media_port,
        &media_base,
        &media_state_db,
        &provider_base,
        &media_log,
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, control_plane, &gateway_state_db),
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
    assert_ready_profiles(&gateway_base, 1).await?;

    let token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "media:use",
            "--group",
            "engineering",
            "--role",
            "operator",
            "--data-label",
            "cui",
        ],
    )?;
    let token = token.trim();

    let cancel_output = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "run".into(),
            "fake/image".into(),
            "--tool-name".into(),
            "media__run".into(),
            "--input".into(),
            r#"{"prompt":"cancel"}"#.into(),
            "--cancel".into(),
        ],
    )?;
    let cancel_task_id = task_id_from_output(&cancel_output)?;
    contains(
        &cancel_output,
        &format!("cancelled task {cancel_task_id} (status Cancelled)"),
    )?;
    contains(&cancel_output, "  [resource list changed]")?;
    contains(
        &cancel_output,
        &format!("  [task {cancel_task_id}] Working: submitted; prediction"),
    )?;

    let complete_output = run_mcp(
        conformance,
        &gateway_base,
        token,
        ["complete".into(), "fake".into()],
    )?;
    contains(&complete_output, "fake/image")?;

    let run_output = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "run".into(),
            "fake/image".into(),
            "--tool-name".into(),
            "media__run".into(),
            "--input".into(),
            r#"{"prompt":"smoke"}"#.into(),
            "--output-dir".into(),
            output_dir.as_os_str().to_os_string(),
        ],
    )?;
    let task_id = task_id_from_output(&run_output)?;
    for expected in [
        "  [resource list changed]".to_string(),
        format!("  [task {task_id}] Working: submitted; prediction"),
        "  [resource updated] media://prediction/".to_string(),
        format!("  [task {task_id}] Completed: completed;"),
        "subscribed to media://prediction/".to_string(),
        "unsubscribed from media://prediction/".to_string(),
    ] {
        contains(&run_output, &expected)?;
    }

    let structured: SmokeGenerationRunOutput = structured_from_output(&run_output)?;
    if structured.artifacts.is_empty() {
        bail!("run output had no artifacts: {run_output}");
    }
    for artifact in &structured.artifacts {
        if artifact.metadata.get("task_id").and_then(Value::as_str) != Some(task_id.as_str()) {
            bail!("artifact metadata did not use task id `{task_id}`: {artifact:?}");
        }
        if artifact.compliance.tenant_id.as_deref() != Some("tenant-a")
            || !artifact
                .compliance
                .data_labels
                .iter()
                .any(|label| label == "cui")
        {
            bail!("artifact compliance labels were not propagated: {artifact:?}");
        }
    }
    assert_output_file(&output_dir, "png")?;

    let usage = wait_for_actual_usage(
        conformance,
        &format!("{gateway_base}/mcp/default"),
        &task_id,
        Some(token),
    )?;
    assert_usage_report(&usage, "media", &task_id)?;

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
    assert_no_audit_denies(&audit_summary)?;
    assert_audit_method(&audit_summary, "completion/complete", 1, 0)?;
    assert_audit_method(&audit_summary, "tools/call", 2, 0)?;
    assert_audit_method(&audit_summary, "tasks/cancel", 1, 0)?;
    assert_audit_method(&audit_summary, "tasks/get", 2, 0)?;
    assert_audit_method(&audit_summary, "tasks/result", 2, 0)?;
    assert_audit_method(&audit_summary, "resources/subscribe", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/unsubscribe", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/read", 2, 0)?;

    media_child.stop();
    provider.stop();
    cleanup.remove_on_drop();
    println!("gateway task run smoke ok");
    Ok(())
}
