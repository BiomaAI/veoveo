//! Speech must retain hardware admission and private session ownership.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Write, process::Command};

fn render(values: Value) -> Result<std::process::Output> {
    let mut file = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(file.as_file_mut(), &values)?;
    file.flush()?;
    Ok(Command::new("timeout")
        .args(["25s", "helm", "template", "speech-test", "deploy/helm/veoveo",
            "--set", "gateway.controlPlaneRevision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"])
        .arg("--values").arg(file.path())
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).output()?)
}

#[test]
fn speech_admits_hardware_and_separates_readiness() -> Result<()> {
    let output = render(json!({}))?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let objects: Vec<Value> =
        serde_yaml_ng::Deserializer::from_str(std::str::from_utf8(&output.stdout)?)
            .map(Value::deserialize)
            .collect::<Result<_, _>>()?;
    let deployment = objects
        .iter()
        .find(|v| v["kind"] == "Deployment" && v["metadata"]["name"] == "speech-mcp")
        .context("Speech deployment")?;
    ensure!(deployment["spec"]["replicas"] == 1);
    let pod = &deployment["spec"]["template"]["spec"];
    ensure!(pod["runtimeClassName"] == "nvidia");
    ensure!(pod["automountServiceAccountToken"] == false);
    let container = &pod["containers"][0];
    ensure!(container["resources"]["requests"]["nvidia.com/gpu"] == "1");
    ensure!(container["resources"]["limits"]["nvidia.com/gpu"] == "1");
    ensure!(container["readinessProbe"]["httpGet"]["path"] == "/speech/readyz");
    ensure!(container["livenessProbe"]["httpGet"]["path"] == "/speech/healthz");
    ensure!(container["securityContext"]["readOnlyRootFilesystem"] == true);
    let env = container["env"].as_array().context("environment")?;
    ensure!(
        env.iter()
            .filter(
                |v| ["VEOVEO_SURREAL_PASSWORD", "VEOVEO_INTERNAL_TRUST_JWKS"]
                    .contains(&v["name"].as_str().unwrap_or_default())
            )
            .all(|v| v["valueFrom"]["secretKeyRef"].is_object() && v.get("value").is_none())
    );
    ensure!(
        pod["volumes"]
            .as_array()
            .context("volumes")?
            .iter()
            .any(|v| v["name"] == "tmp" && v["emptyDir"]["sizeLimit"] == "6Gi")
    );
    Ok(())
}

#[test]
fn speech_rejects_gpu_removal_and_multiple_owners() -> Result<()> {
    for values in [
        json!({"domainServiceReplicas":{"speech-mcp":2}}),
        json!({"domainServiceResources":{"speech-mcp":{"requests":{"cpu":"1","memory":"2Gi"},"limits":{"cpu":"4","memory":"8Gi"}}}}),
        json!({"domainServiceResources":{"speech-mcp":{"requests":{"nvidia.com/gpu":"0"},"limits":{"nvidia.com/gpu":"0"}}}}),
    ] {
        ensure!(
            !render(values)?.status.success(),
            "unsafe Speech deployment accepted"
        );
    }
    Ok(())
}
