use super::*;

pub(crate) async fn gateway_chart_projection(
    conformance: &Path,
    gateway: &Path,
    base_control_plane: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(gateway)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let chart_port = 18816u16;
    let gateway_port = 18817u16;
    let chart_base = format!("http://127.0.0.1:{chart_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let generated_control_plane = tmpdir.join("gateway.chart-projection.json");
    let chart_log = tmpdir.join("chart-fixture.log");
    let gateway_log = tmpdir.join("gateway.log");
    let chart_ready = tmpdir.join("chart.ready");

    let mut chart = spawn_fake_hosted_mcp(
        conformance,
        chart_port,
        "charts",
        "vendor",
        &chart_ready,
        &chart_log,
    )?;
    wait_for_file_and_http(&chart_ready, &format!("{chart_base}/charts/healthz")).await?;

    write_chart_control_plane(
        base_control_plane,
        &generated_control_plane,
        &format!("{chart_base}/charts/mcp"),
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
    contains(&validation, "ok: 2 server(s), 2 profile(s)")?;

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

    let token = run_checked(
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
    let token = token.trim();
    let info = run_mcp(conformance, &gateway_base, token, ["info".into()])?;
    contains(&info, "tool `charts__render_chart`")?;
    contains(&info, "tool `charts__create_chart_view`")?;
    contains(&info, "prompt `author_chart`")?;

    let resources = run_mcp(conformance, &gateway_base, token, ["resources".into()])?;
    contains(&resources, "charts://chart-types")?;
    contains(&resources, "ui://charts/chart-view.html")?;

    let call = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "call".into(),
            "--tool-name".into(),
            "charts__create_chart_view".into(),
            "--arguments".into(),
            r#"{"chart_type":"bar","data":[{"label":"a","value":1}]}"#.into(),
        ],
    )?;
    contains(&call, "charts fixture rendered chart view")?;
    assert_structured_field(&call, "chart_types_uri", "charts://chart-types")?;
    assert_structured_field(&call, "view_resource_uri", "ui://charts/chart-view.html")?;

    let chart_types = run_mcp(
        conformance,
        &gateway_base,
        token,
        ["resource".into(), "charts://chart-types".into()],
    )?;
    let chart_types: Value = serde_json::from_str(&chart_types)?;
    if chart_types.get("server").and_then(Value::as_str) != Some("charts")
        || chart_types
            .get("types")
            .and_then(Value::as_array)
            .map(Vec::len)
            != Some(2)
    {
        bail!("chart types resource was not routed correctly: {chart_types}");
    }

    let chart_view = run_mcp(
        conformance,
        &gateway_base,
        token,
        ["resource".into(), "ui://charts/chart-view.html".into()],
    )?;
    let chart_view: Value = serde_json::from_str(&chart_view)?;
    if chart_view.get("server").and_then(Value::as_str) != Some("charts")
        || chart_view.get("kind").and_then(Value::as_str) != Some("chart_view")
    {
        bail!("chart UI resource was not routed correctly: {chart_view}");
    }

    let prompt = run_mcp(
        conformance,
        &gateway_base,
        token,
        [
            "prompt".into(),
            "author_chart".into(),
            "--arguments".into(),
            r#"{"chart_type":"bar"}"#.into(),
        ],
    )?;
    contains(&prompt, "Author a bar chart")?;

    gateway_child.stop();
    chart.stop();
    cleanup.remove_on_drop();
    println!("gateway chart projection smoke ok");
    Ok(())
}

fn write_chart_control_plane(base: &Path, output: &Path, upstream_url: &str) -> Result<()> {
    let mut control_plane: Value = serde_json::from_str(&fs::read_to_string(base)?)?;
    control_plane_array_mut(&mut control_plane, "servers")?.push(serde_json::json!({
        "slug": "charts",
        "uri_scheme": "charts",
        "mount_path": "/charts",
        "mcp_path": "/charts/mcp",
        "upstream": {
            "transport": "streamable_http",
            "url": upstream_url,
            "security": "loopback_http"
        },
        "capabilities": {
            "tools": true,
            "resources": true,
            "resource_templates": false,
            "resource_subscriptions": false,
            "prompts": true,
            "completions": false,
            "tasks": false,
            "notifications": false
        },
        "resource_projection": "server_owned",
        "tools": ["render_chart", "create_chart_view"],
        "prompts": ["author_chart"],
        "required_scopes": ["operator:use"],
        "owned_routes": [],
        "metadata": {}
    }));
    let operator = control_plane_array_mut(&mut control_plane, "profiles")?
        .iter_mut()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some("operator"))
        .ok_or_else(|| anyhow!("control plane has no operator profile"))?;
    operator["servers"] = serde_json::json!([{
        "server": "charts",
        "tools": {
            "mode": "listed",
            "items": ["render_chart", "create_chart_view"]
        },
        "resources": {
            "mode": "listed",
            "items": [
                { "kind": "scheme", "scheme": "charts" },
                { "kind": "uri_prefix", "prefix": "ui://charts/" }
            ]
        },
        "prompts": {
            "mode": "listed",
            "items": ["author_chart"]
        },
        "completions": "disabled",
        "tasks": "disabled"
    }]);
    let rules = control_plane_array_mut(&mut control_plane, "policies")?
        .iter_mut()
        .find(|policy| policy.get("version").and_then(Value::as_str) == Some("2026-07-02"))
        .and_then(|policy| policy.get_mut("rules"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no 2026-07-02 policy rules"))?;
    rules.push(serde_json::json!({
        "id": "allow_operator_charts_use",
        "effect": "allow",
        "actions": [
            "tools_list",
            "tools_call",
            "resources_list",
            "resources_read",
            "prompts_list",
            "prompts_get"
        ],
        "profiles": ["operator"],
        "servers": ["charts"],
        "tools": ["render_chart", "create_chart_view"],
        "resource_schemes": ["charts", "ui"],
        "prompts": ["author_chart"],
        "required_scopes": ["operator:use"],
        "metadata": {}
    }));

    let parsed: veoveo_mcp_contract::GatewayControlPlane =
        serde_json::from_value(control_plane.clone())?;
    parsed.validate()?;
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}

fn control_plane_array_mut<'a>(
    control_plane: &'a mut Value,
    key: &str,
) -> Result<&'a mut Vec<Value>> {
    control_plane
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no `{key}` array"))
}
