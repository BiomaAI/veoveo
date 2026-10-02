//! Bundled storage worker configuration and traffic admission through real Helm.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::process::{Command, Output};

fn render(workers: Option<&str>) -> Result<Output> {
    let mut command = Command::new("timeout");
    command.args([
        "25s",
        "helm",
        "template",
        "object-store-test",
        "deploy/helm/veoveo",
        "--values",
        "testing/fixtures/platform-selection/platform-values.yaml",
    ]);
    if let Some(workers) = workers {
        command.args([
            "--set-json",
            &format!("objectStore.rustfs.runtimeWorkerThreads={workers}"),
        ]);
    }
    command
        .output()
        .context("rendering object store configuration")
}

pub(super) fn check() -> Result<()> {
    for (configured, expected) in [(None, "4"), (Some("8"), "8")] {
        let output = render(configured)?;
        ensure!(
            output.status.success(),
            "object store rendering failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let objects = serde_yaml_ng::Deserializer::from_slice(&output.stdout)
            .map(Value::deserialize)
            .collect::<Result<Vec<_>, _>>()?;
        let store = objects
            .iter()
            .find(|o| o["kind"] == "StatefulSet" && o["metadata"]["name"] == "rustfs")
            .context("rendered RustFS StatefulSet")?;
        let container = &store["spec"]["template"]["spec"]["containers"][0];
        let workers = container["env"]
            .as_array()
            .context("RustFS environment")?
            .iter()
            .find(|v| v["name"] == "RUSTFS_RUNTIME_WORKER_THREADS")
            .context("explicit RustFS runtime workers")?;
        ensure!(workers["value"] == expected);
        ensure!(container["resources"]["limits"]["cpu"] == "1");
        ensure!(container["readinessProbe"]["httpGet"]["path"] == "/health/ready");
        ensure!(container["livenessProbe"]["httpGet"]["path"] == "/health");
    }
    for invalid in ["0", "1", "1.5", "\"4\""] {
        let output = render(Some(invalid))?;
        ensure!(
            !output.status.success(),
            "object store accepted invalid worker count {invalid}"
        );
        ensure!(
            String::from_utf8_lossy(&output.stderr).contains("runtimeWorkerThreads"),
            "worker count failed outside its schema gate"
        );
    }
    println!("RustFS uses explicit async workers and storage readiness for traffic admission.");
    Ok(())
}
