//! Real Helm rendering qualifies the core/configured boundary, without a cluster.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::process::Command;

fn render(extra: &[&str]) -> std::process::Output {
    Command::new("helm")
        .args([
            "template",
            "computers-test",
            "deploy/helm/veoveo",
            "--namespace",
            "computers-test",
            "--set",
            "gateway.controlPlaneRevision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ])
        .args(extra)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("Helm must be installed for configuration qualification")
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

fn object<'a>(objects: &'a [Value], kind: &str, name: &str) -> Result<&'a Value> {
    objects
        .iter()
        .find(|o| o["kind"] == kind && o["metadata"]["name"] == name)
        .context("expected rendered Computers object")
}

#[test]
fn core_presets_render_stable_unconfigured_control_without_privileged_capacity() -> Result<()> {
    for preset in ["full", "extension-foundation"] {
        let option = format!("installationPreset={preset}");
        let first = objects(render(&["--set", &option]))?;
        let second = objects(render(&["--set", &option]))?;
        let deployment = object(&first, "Deployment", "computers-mcp")?;
        ensure!(
            deployment == object(&second, "Deployment", "computers-mcp")?,
            "no-op render changes pod template"
        );
        ensure!(deployment["spec"]["replicas"] == 2);
        let pod = &deployment["spec"]["template"]["spec"];
        ensure!(pod["automountServiceAccountToken"] == false);
        let container = &pod["containers"][0];
        ensure!(container["securityContext"]["allowPrivilegeEscalation"] == false);
        ensure!(container["securityContext"]["readOnlyRootFilesystem"] == true);
        ensure!(container["readinessProbe"]["httpGet"]["path"] == "/computers/healthz");
        ensure!(
            container["readinessProbe"]["httpGet"]["httpHeaders"][0]["value"]
                == "computers-mcp:8804"
        );
        ensure!(
            pod["volumes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v.get("hostPath").is_none())
        );
        let config = object(&first, "ConfigMap", "computers-configuration")?;
        ensure!(config == object(&second, "ConfigMap", "computers-configuration")?);
        let config: Value =
            serde_json::from_str(config["data"]["computers.json"].as_str().unwrap())?;
        ensure!(config["capacity"]["kind"] == "unconfigured");
        ensure!(config["allowedOrigins"][0] == "https://veoveo.enterprise.example");
        let id = config["providerInstanceId"].as_str().unwrap();
        ensure!(id.len() == 36 && &id[14..15] == "8");
    }
    Ok(())
}

#[test]
fn configured_capacity_requires_explicit_configuration_and_trust() -> Result<()> {
    let missing = render(&["--set", "computers.capacityMode=openshell-docker"]);
    ensure!(!missing.status.success());
    ensure!(String::from_utf8_lossy(&missing.stderr).contains("computers.existingConfigMap"));
    let revision = format!("computers.configurationRevision={}", "a".repeat(64));
    let configured = objects(render(&[
        "--set",
        "computers.capacityMode=openshell-docker",
        "--set",
        "computers.existingConfigMap=admitted-computers",
        "--set",
        "computers.existingTrustSecret=computers-worker-trust",
        "--set",
        &revision,
    ]))?;
    ensure!(object(&configured, "ConfigMap", "computers-configuration").is_err());
    let deployment = object(&configured, "Deployment", "computers-mcp")?;
    let volumes = deployment["spec"]["template"]["spec"]["volumes"]
        .as_array()
        .unwrap();
    ensure!(
        volumes
            .iter()
            .any(|v| v["configMap"]["name"] == "admitted-computers")
    );
    ensure!(
        volumes
            .iter()
            .any(|v| v["secret"]["secretName"] == "computers-worker-trust")
    );
    let invalid = render(&["--set", "computers.existingConfigMap=ignored-configuration"]);
    ensure!(
        !invalid.status.success(),
        "unconfigured mode silently ignored external configuration"
    );
    let no_store = render(&[
        "--set",
        "installationPreset=custom",
        "--set",
        "components={gateway}",
        "--set",
        "mcpServers={computers}",
    ]);
    ensure!(!no_store.status.success());
    ensure!(
        String::from_utf8_lossy(&no_store.stderr).contains("requires component platform-store")
    );
    Ok(())
}
