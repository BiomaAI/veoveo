//! Native rendered-manifest checks. Requires Helm and GNU timeout; no GPU claims.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Write, process::Command};

fn render(values: &Value) -> Result<std::process::Output> {
    let mut file = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(file.as_file_mut(), values)?;
    file.flush()?;
    Command::new("timeout")
        .args(["25s", "helm", "template", "embedding-test", "deploy/helm/veoveo", "--namespace", "platform", "--set", "gateway.auditRetentionDays=1", "--set", "gateway.controlPlaneRevision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"])
        .arg("--values").arg(file.path())
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output().context("embedding chart checks require Helm and GNU timeout")
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
        .find(|v| v["kind"] == kind && v["metadata"]["name"] == name)
        .context("expected embedding object")
}
fn selection() -> Value {
    json!({"installationPreset":"custom", "components":["embedding-runtime"], "mcpServers":[], "networkPolicy":{"enabled":false}})
}

#[test]
fn runtime_uses_the_qualified_pin_verified_checkpoint_gpu_and_secret_reference() -> Result<()> {
    let rendered = objects(render(&selection())?)?;
    let deployment = find(&rendered, "Deployment", "embedding")?;
    ensure!(deployment["spec"]["strategy"]["type"] == "Recreate");
    let pod = &deployment["spec"]["template"]["spec"];
    ensure!(pod["runtimeClassName"] == "nvidia");
    ensure!(pod["automountServiceAccountToken"] == false);
    let container = &pod["containers"][0];
    let image = container["image"].as_str().context("image")?;
    let source = include_str!("../../../servers/reason-mcp/Dockerfile");
    let reason = source
        .lines()
        .find_map(|line| line.strip_prefix("ARG VLLM_IMAGE="))
        .context("Reason vLLM pin")?;
    ensure!(
        image.split_once('@').context("embedding digest")?.1
            == reason.split_once('@').context("Reason digest")?.1
    );
    let offline: Value =
        serde_json::from_str(include_str!("../../../deploy/offline/images.lock.json"))?;
    ensure!(
        offline["external_images"]
            .as_array()
            .context("offline images")?
            .iter()
            .any(|entry| entry["source"] == image)
    );
    ensure!(container["resources"]["requests"]["nvidia.com/gpu"] == "1");
    ensure!(container["resources"]["limits"]["nvidia.com/gpu"] == "1");
    let args = container["args"].as_array().context("runtime args")?;
    ensure!(args.iter().any(|v| {
        v.as_str()
            .is_some_and(|s| s.contains("torch.cuda.is_available()"))
    }));
    ensure!(
        args.windows(2)
            .any(|v| v == [json!("--runner"), json!("pooling")])
    );
    ensure!(
        args.windows(2)
            .any(|v| v == [json!("--scheduling-policy"), json!("priority")])
    );
    let env = container["env"].as_array().context("runtime environment")?;
    let key = env
        .iter()
        .find(|v| v["name"] == "VLLM_API_KEY")
        .context("runtime key")?;
    ensure!(key.get("value").is_none());
    ensure!(
        key["valueFrom"]["secretKeyRef"] == json!({"name":"veoveo-embedding", "key":"api-key"})
    );
    ensure!(
        env.iter()
            .any(|v| v["name"] == "HF_HUB_OFFLINE" && v["value"] == "1")
    );
    ensure!(container["readinessProbe"]["httpGet"]["path"] == "/health");
    ensure!(pod["initContainers"][0]["image"] == image);
    ensure!(
        pod["initContainers"][0]["args"][0]
            .as_str()
            .context("init command")?
            .contains("sha256sum --check --strict")
    );
    let manifest = include_str!("../../../platform/runtimes/embedding/checkpoint.sha256");
    ensure!(
        include_str!("../../../deploy/helm/veoveo/files/embedding-checkpoint.sha256") == manifest
    );
    ensure!(
        find(&rendered, "ConfigMap", "embedding-checkpoint")?["data"]["checkpoint.sha256"]
            == manifest
    );
    ensure!(
        find(&rendered, "PersistentVolumeClaim", "embedding-model-cache")?["spec"]["resources"]["requests"]
            ["storage"]
            == "4Gi"
    );
    Ok(())
}

#[test]
fn isolation_is_always_present_and_no_general_ingress_policy_bypasses_it() -> Result<()> {
    for general_policy in [false, true] {
        let mut values = selection();
        values["networkPolicy"]["enabled"] = json!(general_policy);
        let rendered = objects(render(&values)?)?;
        let policy = find(&rendered, "NetworkPolicy", "embedding")?;
        ensure!(policy["spec"]["policyTypes"] == json!(["Ingress", "Egress"]));
        ensure!(policy["spec"]["egress"] == json!([]));
        let peer = &policy["spec"]["ingress"][0]["from"][0];
        ensure!(peer.get("namespaceSelector").is_none());
        ensure!(
            peer["podSelector"]["matchExpressions"]
                == json!([{"key":"app.kubernetes.io/component","operator":"NotIn","values":["computer-host"]}])
        );
        ensure!(policy["spec"]["ingress"][0]["ports"][0]["port"] == 8000);
        if general_policy {
            let internal = rendered
                .iter()
                .find(|o| {
                    o["kind"] == "NetworkPolicy"
                        && o["metadata"]["name"]
                            .as_str()
                            .is_some_and(|s| s.ends_with("-internal"))
                })
                .context("general internal policy")?;
            ensure!(
                internal["spec"]["podSelector"]["matchExpressions"][0]["values"]
                    .as_array()
                    .context("exclusions")?
                    .contains(&json!("embedding"))
            );
        }
    }
    Ok(())
}

#[test]
fn renderer_rejects_unpinned_or_cpu_configuration_and_accepts_declared_gpu_claim() -> Result<()> {
    for value in [
        json!({"runtimeClassName":"runc"}),
        json!({"image":{"digest":""}}),
        json!({"resources":{"requests":{"nvidia.com/gpu":"0"}}}),
        json!({"engine":{"gpuMemoryUtilization":0}}),
        json!({"apiKeySecret":{"value":"inline-key"}}),
    ] {
        let mut values = selection();
        values["embedding"] = value;
        ensure!(!render(&values)?.status.success());
    }
    let mut values = selection();
    values["global"] = json!({"gpuPlacement":{"enabled":true,"claimName":"embedding-gpu","runtimeClassName":"nvidia","evidenceDigest":format!("sha256:{}","a".repeat(64)),"workloadRequests":{"embedding":"embedding"},"workloadReplicas":{"embedding":1}}});
    let rendered = objects(render(&values)?)?;
    let pod = &find(&rendered, "Deployment", "embedding")?["spec"]["template"]["spec"];
    ensure!(pod["resourceClaims"][0]["resourceClaimName"] == "embedding-gpu");
    ensure!(
        pod["containers"][0]["resources"]["claims"][0]
            == json!({"name":"veoveo-gpu","request":"embedding"})
    );
    ensure!(
        pod["containers"][0]["resources"]["limits"]
            .get("nvidia.com/gpu")
            .is_none()
    );
    Ok(())
}
