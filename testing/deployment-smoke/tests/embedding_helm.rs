//! Native rendered-manifest checks. Requires Helm and GNU timeout; no GPU claims.
mod support;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    process::Command,
};

fn render(values: &Value) -> Result<std::process::Output> {
    let plan = support::module_plan()?;
    let mut file = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(file.as_file_mut(), values)?;
    file.flush()?;
    Command::new("timeout")
        .args(["25s", "helm", "template", "embedding-test", "deploy/helm/veoveo", "--namespace", "platform", "--set", "gateway.auditRetentionDays=1", "--set", "gateway.controlPlaneRevision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"])
        .arg("--set-file")
        .arg(format!("moduleInstallation.planJson={}", plan.path().display()))
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
        offline["externalImages"]
            .as_array()
            .context("offline images")?
            .iter()
            .any(|entry| entry["source"] == image)
    );
    ensure!(container["resources"]["requests"]["nvidia.com/gpu"] == "1");
    ensure!(container["resources"]["limits"]["nvidia.com/gpu"] == "1");
    let args = container["args"].as_array().context("runtime args")?;
    ensure!(args.iter().filter(|arg| **arg == json!("--dtype")).count() == 1);
    ensure!(
        args.windows(2)
            .any(|pair| pair == [json!("--dtype"), json!("bfloat16")])
    );
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
fn declared_precision_is_passed_once_to_the_pooling_engine() -> Result<()> {
    for precision in ["bfloat16", "float16"] {
        let mut values = selection();
        values["embedding"] = json!({"engine":{"precision":precision}});
        let rendered = objects(render(&values)?)?;
        let args =
            find(&rendered, "Deployment", "embedding")?["spec"]["template"]["spec"]["containers"]
                [0]["args"]
                .as_array()
                .context("runtime args")?;
        ensure!(args.iter().filter(|arg| **arg == json!("--dtype")).count() == 1);
        ensure!(
            args.windows(2)
                .any(|pair| pair == [json!("--dtype"), json!(precision)])
        );
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
    for precision in [
        json!("auto"),
        json!("float32"),
        json!("int8"),
        json!("int4"),
        json!("unknown"),
        json!("fp16"),
        json!("BFLOAT16"),
        json!(16),
        json!(null),
        json!([]),
        json!({}),
    ] {
        let mut values = selection();
        values["embedding"] = json!({"engine":{"precision":precision}});
        let output = render(&values)?;
        ensure!(
            !output.status.success(),
            "unsupported precision admitted: {precision}"
        );
        ensure!(
            String::from_utf8_lossy(&output.stderr).contains("precision"),
            "precision refusal must identify its configuration field"
        );
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

#[test]
fn reference_staging_preserves_active_claims_and_omits_deferred_claims() -> Result<()> {
    let repository = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let reference = objects(
        Command::new("timeout")
            .args(["25s", "kubectl", "kustomize", "examples/bioma"])
            .current_dir(repository)
            .output()
            .context("reference Kustomize inputs")?,
    )?;
    let release = find(&reference, "HelmRelease", "veoveo")?;
    let namespace = release["metadata"]["namespace"]
        .as_str()
        .context("release namespace")?;
    let directory = tempfile::tempdir()?;
    let mut helm = Command::new("timeout");
    helm.args(["25s", "helm", "template"])
        .arg(
            release["spec"]["releaseName"]
                .as_str()
                .context("release name")?,
        )
        .arg("deploy/helm/veoveo")
        .arg("--namespace")
        .arg(
            release["spec"]["targetNamespace"]
                .as_str()
                .context("target namespace")?,
        )
        .current_dir(repository);
    for (index, source) in release["spec"]["valuesFrom"]
        .as_array()
        .context("release values")?
        .iter()
        .enumerate()
    {
        ensure!(
            source["kind"] == "ConfigMap",
            "reference values must use ConfigMaps"
        );
        let config = reference
            .iter()
            .find(|object| {
                object["kind"] == "ConfigMap"
                    && object["metadata"]["name"] == source["name"]
                    && object["metadata"]["namespace"] == namespace
            })
            .context("same-namespace reference values")?;
        ensure!(
            config["immutable"] == true,
            "reference values must be immutable"
        );
        let key = source["valuesKey"].as_str().context("values key")?;
        let file = directory.path().join(format!("values-{index}.yaml"));
        std::fs::write(
            &file,
            config["data"][key]
                .as_str()
                .context("generated values bytes")?,
        )?;
        helm.arg("--values").arg(file);
    }
    let output = helm.output().context("full reference Helm render")?;
    let rendered = output.stdout.clone();
    let _helm_objects = objects(output)?;
    std::fs::write(directory.path().join("rendered.yaml"), rendered)?;
    let renderers = release["spec"]["postRenderers"]
        .as_array()
        .context("reference postrenderers")?;
    ensure!(renderers.len() == 1, "qualify every reference postrenderer");
    let mut kustomization = json!({"apiVersion":"kustomize.config.k8s.io/v1beta1", "kind":"Kustomization", "resources":["rendered.yaml"]});
    // Kubernetes' YAML parser admits octal file modes (0400) differently from the
    // generic YAML reader. Compare both sides through the same native parser.
    std::fs::write(
        directory.path().join("kustomization.yaml"),
        serde_yaml_ng::to_string(&kustomization)?,
    )?;
    let before = objects(
        Command::new("timeout")
            .args(["25s", "kubectl", "kustomize"])
            .arg(directory.path())
            .output()
            .context("unpatched reference Kubernetes manifest")?,
    )?;
    let settings = renderers[0]["kustomize"]
        .as_object()
        .context("reference Kustomize postrenderer")?;
    ensure!(
        settings.keys().all(|key| key == "patches"),
        "unsupported postrenderer settings require qualification"
    );
    kustomization
        .as_object_mut()
        .context("Kustomization object")?
        .extend(settings.clone());
    std::fs::write(
        directory.path().join("kustomization.yaml"),
        serde_yaml_ng::to_string(&kustomization)?,
    )?;
    let after = objects(
        Command::new("timeout")
            .args(["25s", "kubectl", "kustomize"])
            .arg(directory.path())
            .output()
            .context("actual reference postrenderer")?,
    )?;

    let deferred = BTreeSet::from([
        "computer-host-data",
        "reason-model-cache",
        "reason-recording-cache",
        "recording-spool",
        "recording-catalog-cache",
        "stream-model-cache",
        "stream-recording-cache",
        "optimization-mcp-workspace",
    ]);
    let active_claims = ["embedding-model-cache", "map-mcp-workspace"];
    let zero = BTreeSet::from([
        "computer-host",
        "computers-mcp",
        "optimization-mcp",
        "reason-mcp",
        "recording",
        "rerun-bridge",
        "speech-mcp",
        "stream-mcp",
        "view-mcp",
    ]);
    let deadlines = BTreeSet::from(["embedding", "knowledge-mcp"]);
    let before_claims = before
        .iter()
        .filter(|v| v["kind"] == "PersistentVolumeClaim")
        .map(|v| v["metadata"]["name"].as_str().context("claim name"))
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        deferred.is_subset(&before_claims),
        "full reference must render every deferred claim before staging: rendered {before_claims:?}, expected {deferred:?}"
    );
    ensure!(
        before.iter().filter(|v| v["kind"] == "Deployment").count() == 25,
        "full Deployment contract changed"
    );
    ensure!(
        after.iter().filter(|v| v["kind"] == "Deployment").count() == 25,
        "staging must retain all 25 Deployments"
    );

    let identity = |v: &Value| -> Result<(String, String, String, String)> {
        Ok((
            v["apiVersion"].as_str().context("object API")?.into(),
            v["kind"].as_str().context("object kind")?.into(),
            v["metadata"]["namespace"]
                .as_str()
                .unwrap_or_default()
                .into(),
            v["metadata"]["name"]
                .as_str()
                .context("object name")?
                .into(),
        ))
    };
    let after_by_identity = after
        .iter()
        .map(|v| Ok((identity(v)?, v)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    ensure!(
        after_by_identity.len() == after.len(),
        "duplicate postrendered identities"
    );
    for original in &before {
        let name = original["metadata"]["name"]
            .as_str()
            .context("object name")?;
        if original["kind"] == "PersistentVolumeClaim" && deferred.contains(name) {
            continue;
        }
        let actual = after_by_identity
            .get(&identity(original)?)
            .context("staging removed a required object")?;
        let mut expected = original.clone();
        if original["kind"] == "Deployment" {
            if zero.contains(name) {
                expected["spec"]["replicas"] = json!(0);
            }
            if deadlines.contains(name) {
                expected["spec"]["progressDeadlineSeconds"] = json!(1200);
            }
        }
        // Only the active claim's keep annotation may change. Complete labels and existing
        // annotations survive the actual strategic merge, including Helm ownership metadata.
        if original["kind"] == "PersistentVolumeClaim"
            && active_claims.contains(&name)
            && actual["metadata"]["annotations"]["helm.sh/resource-policy"] == "keep"
        {
            if expected["metadata"].get("annotations").is_none() {
                expected["metadata"]["annotations"] = json!({});
            }
            expected["metadata"]["annotations"]["helm.sh/resource-policy"] = json!("keep");
        }
        ensure!(
            expected == **actual,
            "postrenderer changed unapproved object fields for {name}"
        );
    }
    let remaining = after
        .iter()
        .filter(|v| v["kind"] == "PersistentVolumeClaim")
        .map(|v| v["metadata"]["name"].as_str().context("claim name"))
        .collect::<Result<BTreeSet<_>>>()?;
    let still_deferred = remaining
        .intersection(&deferred)
        .copied()
        .collect::<Vec<_>>();
    let missing_keep = active_claims
        .iter()
        .filter(|name| {
            find(&after, "PersistentVolumeClaim", name)
                .map(|v| v["metadata"]["annotations"]["helm.sh/resource-policy"] != "keep")
                .unwrap_or(true)
        })
        .copied()
        .collect::<Vec<_>>();
    ensure!(
        still_deferred.is_empty() && missing_keep.is_empty(),
        "staged release retains deferred PVCs {still_deferred:?}; active claims missing keep {missing_keep:?}"
    );
    ensure!(
        before.len() == after.len() + deferred.len(),
        "only the eight deferred PVC declarations may disappear"
    );
    for name in active_claims {
        let claim = find(&after, "PersistentVolumeClaim", name)?;
        ensure!(claim["metadata"]["labels"]["app.kubernetes.io/managed-by"] == "Helm");
    }
    for workload in &after {
        let (pod, replicas) = match workload["kind"].as_str() {
            Some("Deployment" | "StatefulSet" | "ReplicaSet") => (
                &workload["spec"]["template"]["spec"],
                workload["spec"]
                    .get("replicas")
                    .map(|v| v.as_u64().context("replica count"))
                    .transpose()?
                    .unwrap_or(1),
            ),
            Some("DaemonSet") => (&workload["spec"]["template"]["spec"], 1),
            Some("Job") => (
                &workload["spec"]["template"]["spec"],
                if workload["spec"]["suspend"] == true {
                    0
                } else {
                    workload["spec"]
                        .get("parallelism")
                        .map(|v| v.as_u64().context("Job parallelism"))
                        .transpose()?
                        .unwrap_or(1)
                },
            ),
            Some("Pod") => (&workload["spec"], 1),
            _ => continue,
        };
        if replicas == 0 {
            continue;
        }
        for volume in pod["volumes"].as_array().into_iter().flatten() {
            if let Some(claim) = volume["persistentVolumeClaim"]["claimName"].as_str() {
                ensure!(
                    !deferred.contains(claim),
                    "active workload references an omitted claim"
                );
            }
        }
    }
    Ok(())
}
