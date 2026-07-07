use super::*;

pub(crate) async fn gateway_authenticated(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
    artifact_service: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(gateway)?;
    assert_executable(artifact_service)?;

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

    let plane = spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
        .await?;
    let mut media_child = spawn_media_s3_smoke(
        media,
        media_port,
        PUBLIC_BASE_URL,
        &media_state_db,
        &plane.url,
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
    let control_db = spawn_gateway_control_db(gateway, control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &control_db.url, &gateway_state_db),
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
    assert_ready_profiles(&gateway_base, 2).await?;
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
    let edge_token = gateway_token(conformance, &edge_base, &["--scope", "operator:use"])?;
    run_direct_mcp(
        conformance,
        &format!("{edge_base}/mcp/operator"),
        ["info".into()],
        [("MCP_BEARER_TOKEN", edge_token.trim().into())],
    )?;

    let admin_token = gateway_token_for_profile(
        conformance,
        &gateway_base,
        "admin",
        &["--scope", "operator:use", "--scope", "admin:manage"],
    )?;
    let http = reqwest::Client::new();
    assert_http_get_status(
        &format!("{gateway_base}/admin/admin/control-plane"),
        None,
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let operator_control_plane_token =
        gateway_token(conformance, &gateway_base, &["--scope", "operator:use"])?;
    assert_http_get_status(
        &format!("{gateway_base}/admin/admin/control-plane"),
        Some(operator_control_plane_token.trim()),
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let seeded_control_status = get_json(
        &http,
        &format!("{gateway_base}/admin/admin/control-plane"),
        Some(admin_token.trim()),
    )
    .await?;
    let seeded_revision_id = seeded_control_status
        .get("revision_id")
        .and_then(Value::as_str)
        .filter(|revision_id| revision_id.starts_with("gcp-"))
        .ok_or_else(|| {
            anyhow!("seeded control-plane status had no revision: {seeded_control_status}")
        })?;
    assert_control_plane_status(&seeded_control_status, seeded_revision_id)?;

    let applied = put_json_file(
        &http,
        &format!("{gateway_base}/admin/admin/control-plane"),
        Some(admin_token.trim()),
        control_plane,
    )
    .await?;
    let revision_id = assert_control_plane_admin_result(&applied, "applied")?;
    let control_status = get_json(
        &http,
        &format!("{gateway_base}/admin/admin/control-plane"),
        Some(admin_token.trim()),
    )
    .await?;
    assert_control_plane_status(&control_status, &revision_id)?;

    assert_ready_profiles(&gateway_base, 2).await?;
    let admin_id_jag_token = gateway_id_jag_token_for_profile(
        conformance,
        &gateway_base,
        "admin",
        &[
            "--id-jag-scope",
            "operator:use",
            "--id-jag-scope",
            "admin:manage",
            "--scope",
            "operator:use",
            "--scope",
            "admin:manage",
        ],
    )?;
    let admin_status = get_json(
        &http,
        &format!("{gateway_base}/admin/admin/control-plane"),
        Some(admin_id_jag_token.trim()),
    )
    .await?;
    assert_control_plane_status_with_profiles(&admin_status, &revision_id, 2)?;
    let admin_id_jag_mcp_token = gateway_id_jag_token_for_profile(
        conformance,
        &gateway_base,
        "admin",
        &[
            "--id-jag-scope",
            "operator:use",
            "--id-jag-scope",
            "admin:manage",
            "--scope",
            "operator:use",
            "--scope",
            "admin:manage",
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
        &format!("{gateway_base}/mcp/admin"),
        ["info".into()],
        [("MCP_BEARER_TOKEN", admin_id_jag_mcp_token.trim().into())],
    )?;
    let operator_profile_token =
        gateway_token(conformance, &gateway_base, &["--scope", "operator:use"])?;
    assert_mcp_denied(
        conformance,
        &format!("{gateway_base}/mcp/admin"),
        operator_profile_token.trim(),
        ["info".into()],
    )?;
    let admin_service_token = gateway_token_for_profile(
        conformance,
        &gateway_base,
        "admin",
        &["--scope", "operator:use", "--scope", "admin:manage"],
    )?;
    run_direct_mcp(
        conformance,
        &format!("{gateway_base}/mcp/admin"),
        ["info".into()],
        [("MCP_BEARER_TOKEN", admin_service_token.trim().into())],
    )?;
    assert_mcp_denied(
        conformance,
        &format!("{gateway_base}/mcp/operator"),
        admin_service_token.trim(),
        ["info".into()],
    )?;
    assert_mcp_denied(
        conformance,
        &format!("{gateway_base}/mcp/operator"),
        admin_id_jag_mcp_token.trim(),
        ["info".into()],
    )?;

    gateway_child.stop();
    gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &control_db.url, &gateway_state_db),
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
    assert_ready_profiles(&gateway_base, 2).await?;
    let restarted_admin_status = get_json(
        &http,
        &format!("{gateway_base}/admin/admin/control-plane"),
        Some(admin_id_jag_token.trim()),
    )
    .await?;
    assert_control_plane_status_with_profiles(&restarted_admin_status, &revision_id, 2)?;
    run_direct_mcp(
        conformance,
        &format!("{gateway_base}/mcp/admin"),
        ["info".into()],
        [("MCP_BEARER_TOKEN", admin_service_token.trim().into())],
    )?;

    let token = gateway_token(conformance, &gateway_base, &["--scope", "operator:use"])?;
    let token = token.trim();
    let gateway_mcp_url = format!("{gateway_base}/mcp/operator");
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

    let revoked_token = gateway_token(conformance, &gateway_base, &["--scope", "operator:use"])?;
    let revoked_jti = jwt_id(revoked_token.trim())?;
    let revocation = post_json(
        &http,
        &format!("{gateway_base}/admin/admin/jwt-revocations"),
        Some(admin_token.trim()),
        serde_json::json!({
            "profile": "operator",
            "issuer": "https://veoveo.bioma.ai/oauth",
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
            "operator:use",
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
        &format!("{gateway_base}/admin/admin/control-plane"),
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
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        token,
        [
            "prompt".into(),
            "media-model-select".into(),
            "--arguments".into(),
            r#"{"goal":"choose an image generation model for protected data","media_type":"image","budget":"low"}"#.into(),
        ],
    )?;
    assert_mcp_denied(
        conformance,
        &gateway_mcp_url,
        token,
        ["complete".into(), "fake".into()],
    )?;
    assert_mcp_session_resource_denied(&live_policy_session, "media://usage").await?;
    live_policy_session.cancel().await?;
    let missing_group_token = gateway_id_jag_token(
        conformance,
        &gateway_base,
        &[
            "--id-jag-scope",
            "operator:use",
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
            "operator:use",
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
            "operator:use",
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
        &["--id-jag-scope", "operator:use", "--jwt-id", replay_jti],
    )?;
    if gateway_id_jag_token(
        conformance,
        &gateway_base,
        &["--id-jag-scope", "operator:use", "--jwt-id", replay_jti],
    )
    .is_ok()
    {
        bail!("replayed ID-JAG was unexpectedly accepted");
    }

    if gateway_token(conformance, &gateway_base, &["--scope", "admin:manage"]).is_ok() {
        bail!("operator OAuth client unexpectedly accepted admin:manage scope");
    }

    edge.stop();
    gateway_child.stop();
    let audit_counts = run_gateway_json(gateway, "audit-counts", &gateway_state_db)?;
    assert_json_u64_at_least(&audit_counts, "auth_events", 1)?;
    assert_json_u64_at_least(&audit_counts, "policy_events", 1)?;
    let auth_method_summary =
        run_gateway_json(gateway, "auth-audit-method-summary", &gateway_state_db)?;
    assert_audit_method(&auth_method_summary, "bearer_jwt", 10, 2)?;
    assert_audit_method(
        &auth_method_summary,
        "client_credentials_private_key_jwt",
        4,
        1,
    )?;
    assert_audit_method(&auth_method_summary, "enterprise_managed_id_jag", 5, 1)?;
    let auth_reason_summary =
        run_gateway_json(gateway, "auth-audit-reason-summary", &gateway_state_db)?;
    assert_reason_summary_at_least(&auth_reason_summary, "auth_allow", 10)?;
    assert_reason_summary_at_least(&auth_reason_summary, "missing_authorization_header", 1)?;
    assert_reason_summary_at_least(&auth_reason_summary, "invalid_bearer_token", 3)?;
    assert_reason_summary_at_least(&auth_reason_summary, "invalid_scope", 1)?;
    assert_reason_summary_at_least(&auth_reason_summary, "identity_assertion_replay", 1)?;
    assert_reason_summary_at_least(&auth_reason_summary, "token_revoked", 1)?;
    let auth_principal_kind_summary =
        run_gateway_auth_metadata_summary(gateway, &gateway_state_db, "principal_kind")?;
    assert_metadata_summary_at_least(&auth_principal_kind_summary, "user", 1)?;
    let auth_principal_label_summary =
        run_gateway_auth_metadata_summary(gateway, &gateway_state_db, "principal_data_labels")?;
    assert_metadata_summary_at_least(&auth_principal_label_summary, "cui", 1)?;
    let auth_principal_assurance_summary =
        run_gateway_auth_metadata_summary(gateway, &gateway_state_db, "principal_assurances")?;
    assert_metadata_summary_at_least(&auth_principal_assurance_summary, "us_person", 1)?;
    let audit_summary = run_gateway_json(gateway, "audit-method-summary", &gateway_state_db)?;
    assert_audit_method(&audit_summary, "tools/list", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/list", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/templates/list", 1, 0)?;
    assert_audit_method(&audit_summary, "resources/read", 3, 5)?;
    assert_audit_method(&audit_summary, "prompts/list", 2, 0)?;
    assert_audit_method(&audit_summary, "prompts/get", 1, 1)?;
    assert_audit_method(&audit_summary, "completion/complete", 0, 1)?;
    assert_audit_method(&audit_summary, "tasks/list", 1, 0)?;
    assert_audit_method(&audit_summary, "admin/control-plane", 1, 0)?;
    assert_audit_method(&audit_summary, "admin/control-plane/result", 1, 0)?;
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
