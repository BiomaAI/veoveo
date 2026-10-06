//! Rendered deployment contract; requires Helm and GNU timeout, with no cluster writes.
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
    Command::new("timeout")
        .args(["25s", "helm", "template", "knowledge-test", "deploy/helm/veoveo", "--namespace", "platform",
            "--set", "gateway.auditRetentionDays=1",
            "--set", "gateway.controlPlaneRevision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"])
        .arg("--set-file")
        .arg(format!("moduleInstallation.planJson={}", plan.path().display()))
        .arg("--values").arg(file.path())
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output().context("Knowledge chart checks require Helm and GNU timeout")
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
fn find<'a>(objects: &'a [Value], kind: &str, name: &str) -> Result<&'a Value> {
    objects
        .iter()
        .find(|o| o["kind"] == kind && o["metadata"]["name"] == name)
        .with_context(|| format!("expected {kind} {name}"))
}
fn selection() -> Value {
    json!({
        "installationPreset":"custom",
        "components":["gateway","platform-store","embedding-runtime"],
        "mcpServers":["knowledge"],
        "knowledge":{
            "existingConfigMap":"indexing-public", "existingSigningSecret":"indexing-private",
            "configurationRevision":"b".repeat(64),
            "indexingConfigKeys":["first.json","second.json"],
            "embeddingRuntimeConfigKey":"qualified-runtime.json"
        }
    })
}

#[test]
fn deployment_uses_private_credentials_shared_gpu_and_independent_liveness() -> Result<()> {
    let rendered = objects(render(&selection())?)?;
    let deployment = find(&rendered, "Deployment", "knowledge-mcp")?;
    ensure!(deployment["spec"]["replicas"] == 1);
    ensure!(deployment["spec"]["strategy"]["type"] == "Recreate");
    let pod = &deployment["spec"]["template"]["spec"];
    ensure!(pod["automountServiceAccountToken"] == false);
    ensure!(pod["terminationGracePeriodSeconds"] == 45);
    ensure!(pod.get("runtimeClassName").is_none());
    let container = &pod["containers"][0];
    ensure!(container["image"] == "veoveo/knowledge-mcp:0.1.0");
    ensure!(container["securityContext"]["readOnlyRootFilesystem"] == true);
    ensure!(container["securityContext"]["allowPrivilegeEscalation"] == false);
    ensure!(
        container["resources"]["requests"]
            .get("ephemeral-storage")
            .is_none()
    );
    ensure!(
        container["resources"]["requests"]
            .get("nvidia.com/gpu")
            .is_none()
    );
    ensure!(container["livenessProbe"]["httpGet"]["path"] == "/knowledge/healthz");
    ensure!(container["startupProbe"]["httpGet"]["path"] == "/knowledge/healthz");
    ensure!(container["readinessProbe"]["httpGet"]["path"] == "/knowledge/readyz");
    ensure!(
        container["args"]
            == json!([
                "--allowed-host",
                "knowledge-mcp:8800",
                "--indexing-config",
                "/etc/veoveo/knowledge/config/first.json",
                "--indexing-config",
                "/etc/veoveo/knowledge/config/second.json"
            ])
    );
    let env = container["env"].as_array().context("environment")?;
    let key = env
        .iter()
        .find(|v| v["name"] == "VEOVEO_EMBEDDING_API_KEY")
        .context("embedding credential")?;
    ensure!(key.get("value").is_none());
    ensure!(key["valueFrom"]["secretKeyRef"] == json!({"name":"veoveo-embedding","key":"api-key"}));
    let volumes = pod["volumes"].as_array().context("volumes")?;
    ensure!(volumes.len() == 3);
    let configuration = volumes
        .iter()
        .find(|volume| volume["name"] == "configuration")
        .context("qualified runtime and indexing configuration volume")?;
    ensure!(configuration["configMap"]["name"] == "indexing-public");
    ensure!(
        deployment["spec"]["template"]["metadata"]["annotations"]["checksum/module-plan"]
            .is_string()
    );
    for name in [
        "VEOVEO_MODULE_PLAN",
        "VEOVEO_MODULE_COMPOSITION",
        "VEOVEO_INSTALLATION_GENERATION",
        "VEOVEO_CREDENTIAL_REVISION",
        "VEOVEO_SURREAL_RUNTIME_USERNAME",
    ] {
        ensure!(
            env.iter().any(|value| value["name"] == name),
            "missing module plan input {name}"
        );
    }
    ensure!(env.iter().any(|value| value["name"] == "VEOVEO_MODULE_PLAN"
        && value["value"] == "/etc/veoveo/modules/plan.json"));
    ensure!(volumes.iter().any(|value| {
        value["name"] == "module-plan"
            && value["configMap"]["name"]
                .as_str()
                .is_some_and(|name| name.contains("-module-plan-"))
    }));
    let signing = volumes
        .iter()
        .find(|v| v["name"] == "signing")
        .context("signing volume")?;
    ensure!(signing["secret"]["secretName"] == "indexing-private");
    ensure!(signing["secret"]["defaultMode"] == 0o440);
    ensure!(
        volumes
            .iter()
            .any(|v| v["configMap"]["name"] == "indexing-public")
    );
    ensure!(
        container["volumeMounts"]
            .as_array()
            .context("mounts")?
            .iter()
            .all(|v| v["readOnly"] == true)
    );
    ensure!(!rendered.iter().any(|v| v["kind"] == "Secret"));
    ensure!(!rendered.iter().any(|v| {
        v["kind"] == "PersistentVolumeClaim"
            && v["metadata"]["name"]
                .as_str()
                .is_some_and(|s| s.starts_with("knowledge"))
    }));
    ensure!(
        !rendered
            .iter()
            .any(|v| v["kind"] == "ConfigMap"
                && v["metadata"]["name"] == "knowledge-embedding-space")
    );
    let env = container["env"].as_array().context("environment")?;
    ensure!(
        env.iter()
            .any(|v| v["name"] == "VEOVEO_EMBEDDING_RUNTIME_FILE"
                && v["value"] == "/etc/veoveo/knowledge/config/qualified-runtime.json")
    );
    ensure!(
        !env.iter()
            .any(|v| v["name"] == "VEOVEO_EMBEDDING_SPACE_FILE")
    );
    Ok(())
}

#[test]
fn renderer_rejects_incomplete_authority_and_dependency_configuration() -> Result<()> {
    for patch in [
        json!({"existingConfigMap":""}),
        json!({"embeddingRuntimeConfigKey":""}),
        json!({"existingSigningSecret":""}),
        json!({"configurationRevision":""}),
        json!({"indexingConfigKeys":[]}),
        json!({"indexingConfigKeys":["../signing/private.pem"]}),
        json!({"indexingConfigKeys":["same.json","same.json"]}),
        json!({"privateKey":"inline credentials"}),
        json!({"resources":{"requests":{"nvidia.com/gpu":"1"}}}),
    ] {
        let mut values = selection();
        values["knowledge"]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        ensure!(
            !render(&values)?.status.success(),
            "accepted invalid Knowledge configuration: {patch}"
        );
    }
    for missing in ["gateway", "platform-store", "embedding-runtime"] {
        let mut values = selection();
        values["components"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != missing);
        ensure!(
            !render(&values)?.status.success(),
            "accepted missing {missing}"
        );
    }
    Ok(())
}

#[test]
fn full_selection_includes_knowledge_and_configuration_revision_controls_rollout() -> Result<()> {
    let mut values = selection();
    values["installationPreset"] = json!("full");
    values["components"] = json!([]);
    values["mcpServers"] = json!([]);
    let first = objects(render(&values)?)?;
    let second = objects(render(&values)?)?;
    ensure!(
        find(&first, "Deployment", "knowledge-mcp")?
            == find(&second, "Deployment", "knowledge-mcp")?
    );
    values["knowledge"]["configurationRevision"] = json!("c".repeat(64));
    let changed = objects(render(&values)?)?;
    ensure!(
        find(&first, "Deployment", "knowledge-mcp")?
            != find(&changed, "Deployment", "knowledge-mcp")?
    );
    ensure!(find(&first, "Deployment", "embedding")? == find(&changed, "Deployment", "embedding")?);
    values["installationPreset"] = json!("foundation");
    ensure!(find(&objects(render(&values)?)?, "Deployment", "knowledge-mcp").is_err());
    Ok(())
}

#[test]
fn disabled_gateway_and_knowledge_emit_no_module_plan_or_mount() -> Result<()> {
    let mut values = selection();
    values["components"] = json!(["platform-store"]);
    values["mcpServers"] = json!([]);
    let rendered = objects(render(&values)?)?;
    ensure!(!rendered.iter().any(
        |object| object["metadata"]["labels"]["app.kubernetes.io/component"] == "module-plan"
    ));
    ensure!(find(&rendered, "Deployment", "knowledge-mcp").is_err());
    ensure!(find(&rendered, "Deployment", "gateway").is_err());
    Ok(())
}

#[test]
fn optimization_receives_the_same_revisioned_runtime_plan() -> Result<()> {
    let values = json!({"installationPreset":"custom", "components":["gateway","platform-store","artifact-service","object-store"], "mcpServers":["optimization"]});
    let rendered = objects(render(&values)?)?;
    let deployment = find(&rendered, "Deployment", "optimization-mcp")?;
    let pod = &deployment["spec"]["template"]["spec"];
    let server = pod["containers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "optimization-mcp")
        .unwrap();
    for name in [
        "VEOVEO_MODULE_PLAN",
        "VEOVEO_MODULE_COMPOSITION",
        "VEOVEO_INSTALLATION_GENERATION",
        "VEOVEO_CREDENTIAL_REVISION",
        "VEOVEO_SURREAL_RUNTIME_USERNAME",
    ] {
        ensure!(
            server["env"]
                .as_array()
                .unwrap()
                .iter()
                .any(|env| env["name"] == name),
            "missing {name}"
        );
    }
    ensure!(
        server["volumeMounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|mount| mount["name"] == "module-plan"
                && mount["mountPath"] == "/etc/veoveo/modules"
                && mount["readOnly"] == true)
    );
    ensure!(
        pod["volumes"].as_array().unwrap().iter().any(
            |volume| volume["name"] == "module-plan" && volume["configMap"]["name"].is_string()
        )
    );
    ensure!(
        deployment["spec"]["template"]["metadata"]["annotations"]["checksum/module-plan"]
            .is_string()
    );
    ensure!(server["readinessProbe"].is_object());
    let executor = pod["containers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "cuopt-executor")
        .context("mandatory cuOpt executor")?;
    ensure!(
        matches!(
            executor["resources"]["limits"]["nvidia.com/gpu"].as_str(),
            Some("1")
        ) || executor["resources"]["limits"]["nvidia.com/gpu"] == 1
    );
    Ok(())
}
