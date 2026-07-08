use super::*;

pub(super) fn cmd_gateway_smoke_control_plane(
    base: PathBuf,
    output: PathBuf,
    idp_base_url: String,
    trusted_ca_path: PathBuf,
) -> Result<()> {
    let idp_base = Url::parse(&idp_base_url)?;
    if idp_base.scheme() != "https" || idp_base.host().is_none() {
        return Err(anyhow!("--idp-base-url must be an https URL with a host"));
    }
    let idp_base = idp_base_url.trim_end_matches('/');
    let mut control_plane: Value = serde_json::from_str(&std::fs::read_to_string(&base)?)?;
    let identity_providers = control_plane
        .get_mut("identity_providers")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no identity_providers array"))?;
    let identity_provider = identity_providers
        .iter_mut()
        .find(|provider| provider.get("id").and_then(Value::as_str) == Some("enterprise"))
        .ok_or_else(|| anyhow!("control plane has no `enterprise` identity provider"))?;
    identity_provider["authorization_endpoint"] = json!(format!("{idp_base}/oauth2/authorize"));
    identity_provider["token_endpoint"] = json!(format!("{idp_base}/oauth2/token"));
    identity_provider["enterprise_managed_authorization_endpoint"] =
        json!(format!("{idp_base}/oauth2/id-jag"));
    identity_provider["trusted_certificate_authorities"] = json!([
        {
            "source": "file",
            "path": trusted_ca_path.to_string_lossy()
        }
    ]);

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}

pub(super) fn cmd_gateway_two_server_smoke_control_plane(
    base: PathBuf,
    output: PathBuf,
    media_upstream_url: String,
    simulation_upstream_url: String,
) -> Result<()> {
    validate_loopback_http_url(&media_upstream_url, "--media-upstream-url")?;
    validate_loopback_http_url(&simulation_upstream_url, "--simulation-upstream-url")?;

    let mut control_plane: Value = serde_json::from_str(&std::fs::read_to_string(&base)?)?;
    configure_fake_server(
        &mut control_plane,
        "media",
        "media",
        "/media",
        "/media/mcp",
        &media_upstream_url,
        "media-plan",
    )?;
    append_server_manifest(
        &mut control_plane,
        json!({
            "slug": "simulation",
            "uri_scheme": "simulation",
            "mount_path": "/simulation",
            "mcp_path": "/simulation/mcp",
            "upstream": {
                "transport": "streamable_http",
                "url": simulation_upstream_url,
                "security": "loopback_http"
            },
            "capabilities": fake_hosted_capabilities(),
            "tools": ["run"],
            "prompts": ["simulation-plan"],
            "required_scopes": ["simulation:use"],
            "owned_routes": [],
            "metadata": {}
        }),
    )?;
    configure_operator_profile_for_fake_servers(&mut control_plane)?;
    configure_policy_for_fake_servers(&mut control_plane)?;
    add_scope_to_oauth_clients(&mut control_plane, "simulation:use")?;

    let parsed: GatewayControlPlane = serde_json::from_value(control_plane.clone())?;
    parsed.validate()?;
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}

pub(super) fn cmd_gateway_agent_smoke_control_plane(
    base: PathBuf,
    output: PathBuf,
    duckdb_upstream_url: String,
) -> Result<()> {
    validate_loopback_http_url(&duckdb_upstream_url, "--duckdb-upstream-url")?;

    let mut control_plane: Value = serde_json::from_str(&std::fs::read_to_string(&base)?)?;
    replace_media_server_with_duckdb(&mut control_plane, &duckdb_upstream_url)?;
    configure_profiles_for_duckdb(&mut control_plane)?;
    configure_policy_for_duckdb(&mut control_plane)?;
    drop_media_owned_secrets(&mut control_plane)?;
    drop_media_compatibility_helpers(&mut control_plane)?;

    let parsed: GatewayControlPlane = serde_json::from_value(control_plane.clone())?;
    parsed.validate()?;
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}

fn replace_media_server_with_duckdb(
    control_plane: &mut Value,
    duckdb_upstream_url: &str,
) -> Result<()> {
    let servers = control_plane_array_mut(control_plane, "servers")?;
    let server = servers
        .iter_mut()
        .find(|server| server.get("slug").and_then(Value::as_str) == Some("media"))
        .ok_or_else(|| anyhow!("control plane has no `media` server"))?;
    *server = json!({
        "slug": "duckdb",
        "uri_scheme": "duckdb",
        "mount_path": "/duckdb",
        "mcp_path": "/duckdb/mcp",
        "upstream": {
            "transport": "streamable_http",
            "url": duckdb_upstream_url,
            "security": "loopback_http"
        },
        "capabilities": {
            "tools": true,
            "resources": true,
            "resource_templates": true,
            "resource_subscriptions": false,
            "prompts": false,
            "completions": false,
            "tasks": true,
            "notifications": true
        },
        "tools": ["query", "execute", "ingest", "export"],
        "required_scopes": ["operator:use"],
        "owned_routes": [],
        "metadata": {}
    });
    Ok(())
}

fn configure_profiles_for_duckdb(control_plane: &mut Value) -> Result<()> {
    for profile_id in ["operator", "admin"] {
        let profiles = control_plane_array_mut(control_plane, "profiles")?;
        let profile = profiles
            .iter_mut()
            .find(|profile| profile.get("id").and_then(Value::as_str) == Some(profile_id))
            .ok_or_else(|| anyhow!("control plane has no `{profile_id}` profile"))?;
        profile["servers"] = json!([{
            "server": "duckdb",
            "tools": {
                "mode": "listed",
                "items": ["query", "execute", "ingest", "export"]
            },
            "resources": {
                "mode": "listed",
                "items": [{ "kind": "scheme", "scheme": "duckdb" }]
            },
            "prompts": { "mode": "none" },
            "completions": "disabled",
            "tasks": "enabled"
        }]);
    }
    Ok(())
}

fn drop_media_compatibility_helpers(control_plane: &mut Value) -> Result<()> {
    for client in control_plane_array_mut(control_plane, "oauth_clients")? {
        if let Some(helpers) = client
            .get_mut("allowed_compatibility_helpers")
            .and_then(Value::as_array_mut)
        {
            helpers.retain(|helper| {
                helper
                    .as_str()
                    .is_none_or(|helper| !helper.starts_with("media."))
            });
        }
    }
    Ok(())
}

fn drop_media_owned_secrets(control_plane: &mut Value) -> Result<()> {
    let secrets = control_plane_array_mut(control_plane, "secrets")?;
    secrets.retain(|secret| {
        secret
            .get("owner")
            .and_then(|owner| owner.get("server"))
            .and_then(Value::as_str)
            != Some("media")
    });
    Ok(())
}

fn configure_policy_for_duckdb(control_plane: &mut Value) -> Result<()> {
    let policies = control_plane_array_mut(control_plane, "policies")?;
    let policy = policies
        .first_mut()
        .ok_or_else(|| anyhow!("control plane has no policies"))?;
    let rules = policy
        .get_mut("rules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("policy has no rules array"))?;
    for (rule_id, profile) in [
        ("allow_operator_mcp_use", "operator"),
        ("allow_admin_mcp_use", "admin"),
    ] {
        let rule = rules
            .iter_mut()
            .find(|rule| rule.get("id").and_then(Value::as_str) == Some(rule_id))
            .ok_or_else(|| anyhow!("policy has no `{rule_id}` rule"))?;
        *rule = json!({
            "id": rule_id,
            "effect": "allow",
            "actions": [
                "tools_list",
                "tools_call",
                "resources_list",
                "resources_templates_list",
                "resources_read",
                "tasks_list",
                "tasks_get",
                "tasks_result",
                "tasks_cancel",
                "artifact_read",
                "usage_read"
            ],
            "profiles": [profile],
            "servers": ["duckdb"],
            "tools": ["query", "execute", "ingest", "export"],
            "resource_schemes": ["duckdb"],
            "required_scopes": ["operator:use"],
            "metadata": {}
        });
    }
    Ok(())
}

fn validate_loopback_http_url(value: &str, label: &str) -> Result<()> {
    let url = Url::parse(value)?;
    if url.scheme() != "http" {
        return Err(anyhow!("{label} must use http for loopback smoke"));
    }
    let Some(host) = url.host_str() else {
        return Err(anyhow!("{label} must include a host"));
    };
    if !matches!(host, "127.0.0.1" | "localhost") {
        return Err(anyhow!("{label} must use a loopback host"));
    }
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

fn configure_fake_server(
    control_plane: &mut Value,
    slug: &str,
    scheme: &str,
    mount_path: &str,
    mcp_path: &str,
    upstream_url: &str,
    prompt: &str,
) -> Result<()> {
    let servers = control_plane_array_mut(control_plane, "servers")?;
    let server = servers
        .iter_mut()
        .find(|server| server.get("slug").and_then(Value::as_str) == Some(slug))
        .ok_or_else(|| anyhow!("control plane has no `{slug}` server"))?;
    server["uri_scheme"] = json!(scheme);
    server["mount_path"] = json!(mount_path);
    server["mcp_path"] = json!(mcp_path);
    server["upstream"] = json!({
        "transport": "streamable_http",
        "url": upstream_url,
        "security": "loopback_http"
    });
    server["capabilities"] = fake_hosted_capabilities();
    server["tools"] = json!(["run"]);
    server["prompts"] = json!([prompt]);
    server["owned_routes"] = json!([]);
    Ok(())
}

fn append_server_manifest(control_plane: &mut Value, server: Value) -> Result<()> {
    control_plane_array_mut(control_plane, "servers")?.push(server);
    Ok(())
}

fn fake_hosted_capabilities() -> Value {
    json!({
        "tools": true,
        "resources": true,
        "resource_templates": true,
        "resource_subscriptions": false,
        "prompts": true,
        "completions": true,
        "tasks": false,
        "notifications": false
    })
}

fn configure_operator_profile_for_fake_servers(control_plane: &mut Value) -> Result<()> {
    let profiles = control_plane_array_mut(control_plane, "profiles")?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some("operator"))
        .ok_or_else(|| anyhow!("control plane has no `operator` profile"))?;
    profile["servers"] = json!([
        fake_profile_server_exposure("media", "media"),
        fake_profile_server_exposure("simulation", "simulation"),
    ]);
    Ok(())
}

fn fake_profile_server_exposure(server: &str, scheme: &str) -> Value {
    json!({
        "server": server,
        "tools": {
            "mode": "listed",
            "items": ["run"]
        },
        "resources": {
            "mode": "listed",
            "items": [
                {
                    "kind": "scheme",
                    "scheme": scheme
                }
            ]
        },
        "prompts": {
            "mode": "all"
        },
        "completions": "enabled",
        "tasks": "disabled"
    })
}

fn configure_policy_for_fake_servers(control_plane: &mut Value) -> Result<()> {
    let policies = control_plane_array_mut(control_plane, "policies")?;
    let policy = policies
        .iter_mut()
        .find(|policy| policy.get("version").and_then(Value::as_str) == Some("2026-07-02"))
        .ok_or_else(|| anyhow!("control plane has no `2026-07-02` policy"))?;
    let rules = policy
        .get_mut("rules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("policy has no rules array"))?;
    let media_rule = rules
        .iter_mut()
        .find(|rule| rule.get("id").and_then(Value::as_str) == Some("allow_operator_mcp_use"))
        .ok_or_else(|| anyhow!("policy has no `allow_operator_mcp_use` rule"))?;
    *media_rule = fake_policy_rule(
        "allow_operator_mcp_use",
        "media",
        "media",
        "media-plan",
        "operator:use",
    );
    rules.push(fake_policy_rule(
        "allow_simulation_profile_use",
        "simulation",
        "simulation",
        "simulation-plan",
        "simulation:use",
    ));
    Ok(())
}

fn fake_policy_rule(id: &str, server: &str, scheme: &str, prompt: &str, scope: &str) -> Value {
    json!({
        "id": id,
        "effect": "allow",
        "actions": [
            "tools_list",
            "tools_call",
            "resources_list",
            "resources_templates_list",
            "resources_read",
            "prompts_list",
            "prompts_get",
            "completion_complete"
        ],
        "profiles": ["operator"],
        "servers": [server],
        "tools": ["run"],
        "resource_schemes": [scheme],
        "prompts": [prompt],
        "required_scopes": [scope],
        "metadata": {}
    })
}

fn add_scope_to_oauth_clients(control_plane: &mut Value, scope: &str) -> Result<()> {
    for client in control_plane_array_mut(control_plane, "oauth_clients")? {
        let scopes = client
            .get_mut("allowed_scopes")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow!("OAuth client has no allowed_scopes array"))?;
        if !scopes
            .iter()
            .any(|candidate| candidate.as_str() == Some(scope))
        {
            scopes.push(json!(scope));
        }
    }
    Ok(())
}
