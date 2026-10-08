//! Real Helm rendering of model admission and separate Workspace credentials.
//! Requires Helm and GNU timeout on the Linux development host; no cluster writes.
mod support;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Write, process::Command};

fn render(values: &Value) -> Result<std::process::Output> {
    let plan = support::module_plan()?;
    let mut file = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(file.as_file_mut(), values)?;
    file.flush()?;
    Command::new("timeout").args(["25s", "helm", "template", "workspace-test", "deploy/helm/veoveo", "--namespace", "workspace-test",
        "--set", "gateway.controlPlaneRevision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--set", "gateway.auditRetentionDays=1",
            "--set", "knowledge.existingConfigMap=knowledge-test,knowledge.existingSigningSecret=knowledge-test,knowledge.embeddingRuntimeConfigKey=qualified-runtime.json,knowledge.configurationRevision=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"])
        .arg("--set-file")
        .arg(format!("moduleInstallation.planJson={}", plan.path().display()))
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
    ensure!(
        expected
            .as_array()
            .context("configured agents")?
            .iter()
            .any(|model| model["id"] == "daily-assistant")
    );
    let values = json!({"gateway":{"agents":workspace}, "agentManager":source["agentManager"], "consoleBff":{"workspace":source["consoleBff"]["workspace"]}});
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
    let client = registered["oauthClients"]
        .as_array()
        .context("clients")?
        .iter()
        .find(|client| client["id"] == "workspace")
        .context("Workspace client")?;
    let catalog = veoveo_mcp_gateway::GatewayCatalog::from_control_plane(
        serde_json::from_value(registered.clone())?,
        veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
            .bind(veoveo_gateway_catalog::registry().unwrap())
            .unwrap(),
    )?;
    veoveo_agent_runtime::gateway::ManagedTemplateCatalog::from_json(
        &source["gateway"]["agents"]["templates"].to_string(),
        &catalog,
    )?;
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
    let expected: std::collections::BTreeSet<_> = client["allowedScopes"]
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
                workspace["models"][0]["workContexts"] = json!(["operations", "operations"])
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

/// The chart consumes the same camelCase owner DTOs as the gateway and manager.
#[test]
fn managed_policy_binds_each_current_model_to_its_template_secret() -> Result<()> {
    use veoveo_agent_runtime::contract::authoring::{ModelConnection, RuntimeTemplate};
    let source: Value =
        serde_yaml_ng::from_str(include_str!("../../../examples/bioma/values.yaml"))?;
    let mut template: RuntimeTemplate =
        serde_json::from_value(source["gateway"]["agents"]["templates"][0].clone())?;
    let mut model: ModelConnection = serde_json::from_value(
        source["gateway"]["agents"]["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == "pilot-model")
            .unwrap()
            .clone(),
    )?;
    // Distinct destinations and keys make cross-model mixing observable in the emitted branches.
    model.base_url = "https://first.invalid/v1".into();
    model.model = "first-model".into();
    let mut second = model.clone();
    second.id = serde_json::from_value(json!("second-approved"))?;
    second.api_key = serde_json::from_value(json!("second-reference"))?;
    second.base_url = "https://second.invalid/v1".into();
    second.model = "second-model".into();
    let mut second_binding = template.workload.model_secrets[0].clone();
    second_binding.reference = second.api_key.clone();
    second_binding.secret = "second-secret".into();
    second_binding.key = "second-key".into();
    template.workload.model_secrets[0].secret = "first-secret".into();
    template.workload.model_secrets[0].key = "first-key".into();
    let first_binding = template.workload.model_secrets[0].clone();
    let policy = |template: &RuntimeTemplate,
                  models: &[ModelConnection]|
     -> Result<(String, String)> {
        let values = json!({"gateway":{"agents":{"models":models,"templates":[template],"modelSecrets":source["gateway"]["agents"]["modelSecrets"]}},"agentManager":source["agentManager"]});
        let rendered = objects(render(&values)?)?;
        let pod_policy = rendered
            .iter()
            .find(|o| {
                o["kind"] == "ValidatingAdmissionPolicy"
                    && o["metadata"]["name"]
                        .as_str()
                        .is_some_and(|n| n.ends_with("managed-pods"))
            })
            .context("Pod admission policy")?;
        let environment = pod_policy["spec"]["validations"][4]["expression"]
            .as_str()
            .context("environment admission")?;
        assert!(environment.contains("variables.kernel.env.all(e, variables.kernel.env.filter(other, other.name == e.name).size() == 1) &&"), "rendered uniqueness guard must precede all credential matching: {environment}");
        assert!(
            environment
                .trim_start()
                .starts_with("size(variables.kernel.env) <= "),
            "environment names bound must precede the quadratic uniqueness guard"
        );
        // This is a render-contract assertion; evaluated refusal belongs to the existing native admission test.
        for parameter in template.parameters.values() {
            assert!(
                environment.contains(&json!(parameter.environment_variable).to_string()),
                "{environment}"
            );
        }
        let expression = |suffix: &str, index: usize| -> Result<String> {
            let p = rendered
                .iter()
                .find(|o| {
                    o["kind"] == "ValidatingAdmissionPolicy"
                        && o["metadata"]["name"]
                            .as_str()
                            .is_some_and(|n| n.ends_with(suffix))
                })
                .context("rendered policy")?;
            Ok(p["spec"]["validations"][index]["expression"]
                .as_str()
                .context("policy expression")?
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "))
        };
        Ok((
            expression("managed-deployments", 5)?,
            expression("controller-persistentvolumeclaims", 2)?,
        ))
    };
    let expected_branch =
        |m: &ModelConnection,
         b: &veoveo_agent_runtime::contract::authoring::TemplateSecretBinding| {
            format!(
                "( variables.kernel.env.exists(e, e.name == 'VEOVEO_AGENT_MODEL_URL' && e.value == {}) && variables.kernel.env.exists(e, e.name == 'VEOVEO_AGENT_MODEL_ID' && e.value == {}) && variables.kernel.env.exists(e, e.name == 'VEOVEO_MANAGED_MODEL_KEY' && has(e.valueFrom) && has(e.valueFrom.secretKeyRef) && e.valueFrom.secretKeyRef.name == {} && e.valueFrom.secretKeyRef.key == {}) )",
                json!(m.base_url),
                json!(m.model),
                json!(b.secret),
                json!(b.key)
            )
        };
    let (single, storage) = policy(&template, &[model.clone(), second.clone()])?;
    assert!(
        single.contains(&expected_branch(&model, &first_binding)),
        "{single}"
    );
    assert!(!single.contains("second.invalid"));
    assert!(!single.contains("<nil>"));
    for value in [
        &template.workload.config_map,
        &template.workload.database_secret,
    ] {
        assert!(single.contains(&json!(value).to_string()));
    }
    assert!(single.contains(&format!("quantity(\"{}m\")", template.workload.cpu_millis)));
    assert!(single.contains(&format!("quantity(\"{}Mi\")", template.workload.memory_mib)));
    assert!(storage.contains(&json!(template.workload.storage_class).to_string()));
    assert!(storage.contains(&format!(
        "quantity(\"{}Gi\")",
        template.workload.storage_gib
    )));
    template.models.insert(second.id.clone());
    template.workload.model_secrets.push(second_binding.clone());
    let (multiple, _) = policy(&template, &[model.clone(), second.clone()])?;
    assert!(
        multiple.contains(&format!(
            "{} || {}",
            expected_branch(&model, &first_binding),
            expected_branch(&second, &second_binding)
        )),
        "{multiple}"
    );
    // A foreign API-key reference is never authorized merely because its model is selected.
    second.api_key = serde_json::from_value(json!("foreign-reference"))?;
    let (foreign, _) = policy(&template, &[model.clone(), second.clone()])?;
    assert!(foreign.contains(&expected_branch(&model, &first_binding)));
    assert!(!foreign.contains("second.invalid"));
    template.models.remove(&model.id);
    let (zero, _) = policy(&template, &[model, second])?;
    assert!(zero.ends_with("&& (false) )"), "{zero}");
    assert!(
        !zero.contains("first-secret")
            && !zero.contains("second-secret")
            && !zero.contains("&& ()")
    );
    template.workload.model_secrets.clear();
    let values = json!({"gateway":{"agents":{"templates":[template]}},"agentManager":source["agentManager"]});
    assert!(
        !render(&values)?.status.success(),
        "empty bindings must fail chart admission"
    );
    Ok(())
}
