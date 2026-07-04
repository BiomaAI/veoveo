use super::*;

pub(crate) fn assert_control_plane_admin_result(
    value: &Value,
    expected_status: &str,
) -> Result<String> {
    assert_control_plane_admin_result_with_profiles(value, expected_status, 2)
}

pub(crate) fn assert_control_plane_admin_result_with_profiles(
    value: &Value,
    expected_status: &str,
    expected_profiles: u64,
) -> Result<String> {
    if value.get("status").and_then(Value::as_str) != Some(expected_status)
        || value.get("servers").and_then(Value::as_u64) != Some(1)
        || value.get("profiles").and_then(Value::as_u64) != Some(expected_profiles)
    {
        bail!("unexpected control-plane admin result: {value}");
    }
    let revision_id = value
        .get("revision_id")
        .and_then(Value::as_str)
        .filter(|revision_id| !revision_id.is_empty() && *revision_id != "null")
        .ok_or_else(|| anyhow!("control-plane admin result had no revision id: {value}"))?;
    Ok(revision_id.to_string())
}

pub(crate) fn assert_control_plane_status(value: &Value, expected_revision_id: &str) -> Result<()> {
    assert_control_plane_status_with_profiles(value, expected_revision_id, 2)
}

pub(crate) fn assert_control_plane_status_with_profiles(
    value: &Value,
    expected_revision_id: &str,
    expected_profiles: u64,
) -> Result<()> {
    if value.get("status").and_then(Value::as_str) != Some("ok")
        || value.get("servers").and_then(Value::as_u64) != Some(1)
        || value.get("profiles").and_then(Value::as_u64) != Some(expected_profiles)
        || value.get("revision_id").and_then(Value::as_str) != Some(expected_revision_id)
    {
        bail!("unexpected control-plane status: {value}");
    }
    Ok(())
}

pub(crate) fn jwt_id(token: &str) -> Result<String> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| anyhow!("JWT had no payload segment"))?;
    let payload: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;
    payload
        .get("jti")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("JWT payload had no jti: {payload}"))
}

pub(crate) fn write_cui_control_plane(input: &Path, output: &Path) -> Result<()> {
    let mut control_plane: Value = serde_json::from_str(&fs::read_to_string(input)?)?;
    let policies = control_plane
        .get_mut("policies")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no policies array"))?;
    let policy = policies
        .iter_mut()
        .find(|policy| policy.get("version").and_then(Value::as_str) == Some("2026-07-02"))
        .ok_or_else(|| anyhow!("control plane has no 2026-07-02 policy"))?;
    let rules = policy
        .get_mut("rules")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("policy has no rules array"))?;
    let rule = rules
        .iter_mut()
        .find(|rule| rule.get("id").and_then(Value::as_str) == Some("allow_operator_mcp_use"))
        .ok_or_else(|| anyhow!("policy has no allow_operator_mcp_use rule"))?;
    rule["required_data_labels"] = serde_json::json!(["cui"]);
    rule["required_assurances"] = serde_json::json!(["us_person"]);
    rule["groups"] = serde_json::json!(["engineering"]);
    rule["roles"] = serde_json::json!(["operator"]);
    fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}
