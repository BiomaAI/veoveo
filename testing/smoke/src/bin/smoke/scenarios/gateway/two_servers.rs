use super::*;

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
    let platform_store = spawn_gateway_platform_store(gateway, &generated_control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &platform_store),
        [
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
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
            format!("{gateway_base}/oauth/token").into(),
            "--scope".into(),
            "operator:use".into(),
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
            format!("{gateway_base}/oauth/token").into(),
            "--scope".into(),
            "operator:use".into(),
        ],
        [],
    )?;
    let denied = run_raw(
        conformance,
        [
            "--url".into(),
            format!("{gateway_base}/mcp/operator").into(),
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
    let audit_summary = run_gateway_json(gateway, "audit-method-summary", &platform_store)?;
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
