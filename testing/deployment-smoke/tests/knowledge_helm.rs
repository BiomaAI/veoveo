//! Rendered deployment contract; requires Helm and GNU timeout, with no cluster writes.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Write, process::Command};

fn render(values: &Value) -> Result<std::process::Output> {
    let mut file = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(file.as_file_mut(), values)?;
    file.flush()?;
    Command::new("timeout")
        .args(["25s", "helm", "template", "knowledge-test", "deploy/helm/veoveo", "--namespace", "platform",
            "--set", "gateway.auditRetentionDays=1",
            "--set", "gateway.controlPlaneRevision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"])
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
            "indexingConfigKeys":["first.json","second.json"]
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
    ensure!(container["livenessProbe"]["httpGet"]["path"] == "/knowledge/livez");
    ensure!(container["startupProbe"]["httpGet"]["path"] == "/knowledge/livez");
    ensure!(container["readinessProbe"]["httpGet"]["path"] == "/knowledge/healthz");
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
    let space: Value = serde_json::from_str(
        find(&rendered, "ConfigMap", "knowledge-embedding-space")?["data"]["space.json"]
            .as_str()
            .context("embedding space")?,
    )?;
    let qualified: Value = serde_json::from_str(include_str!(
        "../../../platform/runtimes/embedding/verification/reference.json"
    ))?;
    ensure!(
        space == qualified["space"],
        "rendered embedding identity differs from the qualified reference"
    );
    let runtime =
        &find(&rendered, "Deployment", "embedding")?["spec"]["template"]["spec"]["containers"][0];
    ensure!(
        runtime["image"]
            .as_str()
            .context("runtime image")?
            .ends_with(space["runtimeImage"].as_str().context("runtime digest")?)
    );
    Ok(())
}

#[test]
fn renderer_rejects_incomplete_authority_and_dependency_configuration() -> Result<()> {
    for patch in [
        json!({"existingConfigMap":""}),
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
