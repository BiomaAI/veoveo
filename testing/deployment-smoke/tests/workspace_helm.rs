//! Real Helm rendering of model admission and separate Workspace credentials.
//! Requires Helm and GNU timeout on the Linux development host; no cluster writes.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Write, process::Command};

fn render(values: &Value) -> Result<std::process::Output> {
    let mut file = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(file.as_file_mut(), values)?;
    file.flush()?;
    Command::new("timeout").args(["25s", "helm", "template", "workspace-test", "deploy/helm/veoveo", "--namespace", "workspace-test",
        "--set", "gateway.controlPlaneRevision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"])
        .arg("--values").arg(file.path())
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output().context("Workspace chart qualification requires Helm and GNU timeout")
}

fn objects(output: std::process::Output) -> Result<Vec<Value>> {
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_yaml_ng::Deserializer::from_str(std::str::from_utf8(&output.stdout)?)
        .map(|document| Value::deserialize(document).map_err(Into::into))
        .collect()
}

fn environment<'a>(objects: &'a [Value], name: &str) -> Result<&'a Vec<Value>> {
    objects
        .iter()
        .find(|o| o["kind"] == "Deployment" && o["metadata"]["name"] == name)
        .and_then(|o| o["spec"]["template"]["spec"]["containers"][0]["env"].as_array())
        .context("rendered deployment environment")
}

#[test]
fn workspace_agent_configuration_has_exact_secret_references_and_separate_browser_authority()
-> Result<()> {
    let source: Value =
        serde_yaml_ng::from_str(include_str!("../../../examples/bioma/values.yaml"))?;
    let workspace = source["gateway"]["agents"].clone();
    let expected = &workspace["models"];
    ensure!(expected.as_array().context("configured agents")?.len() == 1);
    let values = json!({"gateway":{"agents":workspace}, "consoleBff":{"workspace":source["consoleBff"]["workspace"]}});
    let rendered = objects(render(&values)?)?;
    let gateway = environment(&rendered, "mcp-gateway")?;
    let config = gateway
        .iter()
        .find(|v| v["name"] == "VEOVEO_AGENT_MODELS")
        .context("agent definitions")?;
    ensure!(serde_json::from_str::<Value>(config["value"].as_str().unwrap())? == *expected);
    let key = gateway
        .iter()
        .find(|v| v["name"] == "VEOVEO_AGENT_MODEL_API_KEY")
        .context("model credential")?;
    ensure!(key.get("value").is_none());
    ensure!(
        key["valueFrom"]["secretKeyRef"]
            == json!({"name":"veoveo-workspace-models","key":"api-key"})
    );
    let edge = environment(&rendered, "console-bff")?;
    ensure!(
        edge.iter()
            .all(|v| v["name"] != "VEOVEO_AGENT_MODEL_API_KEY")
    );
    ensure!(
        edge.iter()
            .any(|v| v["name"] == "VEOVEO_WORKSPACE_OAUTH_CLIENT_ID" && v["value"] == "workspace")
    );
    let scopes = edge
        .iter()
        .find(|v| v["name"] == "VEOVEO_WORKSPACE_OAUTH_SCOPES")
        .context("Workspace scopes")?;
    ensure!(
        scopes["value"]
            .as_str()
            .unwrap()
            .split_whitespace()
            .all(|v| !v.contains("admin"))
    );
    let registered: Value =
        serde_json::from_str(include_str!("../../../examples/bioma/gateway.json"))?;
    let client = registered["oauth_clients"]
        .as_array()
        .context("clients")?
        .iter()
        .find(|client| client["id"] == "workspace")
        .context("Workspace client")?;
    let catalog = veoveo_mcp_gateway::GatewayCatalog::from_control_plane(serde_json::from_value(
        registered.clone(),
    )?)?;
    let profile = registered["profiles"]
        .as_array()
        .context("profiles")?
        .iter()
        .find(|profile| profile["id"] == "workspace")
        .context("Workspace profile")?;
    let supported: std::collections::BTreeSet<String> = catalog
        .profile_supported_scopes(&serde_json::from_value(profile.clone())?)
        .into_iter()
        .map(|scope| scope.to_string())
        .collect();
    let expected: std::collections::BTreeSet<_> = client["allowed_scopes"]
        .as_array()
        .context("registered scopes")?
        .iter()
        .map(|value| value.as_str().unwrap())
        .filter(|scope| supported.contains(*scope))
        .collect();
    let requested: std::collections::BTreeSet<_> = scopes["value"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .collect();
    ensure!(
        requested == expected,
        "Workspace must request the intersection of registered and policy-supported capability scopes"
    );
    let empty = objects(render(&json!({}))?)?;
    ensure!(
        environment(&empty, "mcp-gateway")?
            .iter()
            .any(|v| v["name"] == "VEOVEO_AGENT_MODELS" && v["value"] == "[]")
    );
    Ok(())
}

#[test]
fn workspace_chart_rejects_ambient_credentials_and_unbounded_model_settings() -> Result<()> {
    let source: Value =
        serde_yaml_ng::from_str(include_str!("../../../examples/bioma/values.yaml"))?;
    for field in ["credential", "environment", "budget", "contexts"] {
        let mut workspace = source["gateway"]["agents"].clone();
        match field {
            "credential" => {
                workspace["modelSecrets"]["VEOVEO_AGENT_MODEL_API_KEY"]["value"] =
                    json!("forbidden-inline-fixture")
            }
            "environment" => {
                workspace["modelSecrets"]["VEOVEO_INTERNAL_SIGNING_KEY_ID"] =
                    json!({"existingSecret":"wrong","key":"wrong"})
            }
            "budget" => workspace["models"][0]["limits"]["maxOutputTokens"] = json!(8193),
            "contexts" => {
                workspace["models"][0]["work_contexts"] = json!(["operations", "operations"])
            }
            _ => unreachable!(),
        }
        ensure!(
            !render(&json!({"gateway":{"agents":workspace}}))?
                .status
                .success(),
            "invalid {field} was admitted"
        );
    }
    Ok(())
}
