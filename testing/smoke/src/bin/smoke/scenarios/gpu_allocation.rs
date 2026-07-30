use std::{collections::BTreeSet, io::Write};

use anyhow::ensure;

use super::*;

const GPU_RESOURCE: &str = "nvidia.com/gpu";
const PROBE_PREFIX: &str = "veoveo-gpu-allocation";

struct NamespaceGuard {
    context: String,
    namespace: String,
    active: bool,
}

impl NamespaceGuard {
    fn new(context: &str, namespace: String) -> Self {
        Self {
            context: context.to_owned(),
            namespace,
            active: true,
        }
    }

    fn delete(&mut self) -> Result<()> {
        run_checked(
            Path::new("kubectl"),
            [
                "--context".into(),
                self.context.clone().into(),
                "delete".into(),
                "namespace".into(),
                self.namespace.clone().into(),
                "--ignore-not-found=true".into(),
                "--timeout=2m".into(),
            ],
            [],
        )?;
        self.active = false;
        Ok(())
    }
}

impl Drop for NamespaceGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _ = Command::new("kubectl")
            .args([
                "--context",
                self.context.as_str(),
                "delete",
                "namespace",
                self.namespace.as_str(),
                "--ignore-not-found=true",
                "--wait=false",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub(crate) fn gpu_allocation_verify(
    context: &str,
    node: &str,
    image: &str,
    runtime_class_name: &str,
    timeout: Duration,
) -> Result<()> {
    ensure_digest_pinned_image(image)?;
    ensure!(
        !node.trim().is_empty(),
        "GPU allocation verification requires one exact Kubernetes node name"
    );
    ensure!(
        !runtime_class_name.trim().is_empty(),
        "GPU allocation verification requires a RuntimeClass name"
    );
    verify_node_capacity(context, node)?;

    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let namespace = format!("{PROBE_PREFIX}-{}", &suffix[..12]);
    run_checked(
        Path::new("kubectl"),
        [
            "--context".into(),
            context.into(),
            "create".into(),
            "namespace".into(),
            namespace.clone().into(),
        ],
        [],
    )?;
    let mut namespace_guard = NamespaceGuard::new(context, namespace.clone());

    let pod_names = ["probe-a", "probe-b"];
    let manifest = serde_json::json!({
        "apiVersion": "v1",
        "kind": "List",
        "items": pod_names.map(|name| gpu_probe_pod(
            &namespace,
            name,
            node,
            image,
            runtime_class_name,
        )),
    });
    kubectl_apply_value(context, &manifest)?;

    let timeout_argument = format!("--timeout={}s", timeout.as_secs());
    for pod_name in pod_names {
        run_checked(
            Path::new("kubectl"),
            [
                "--context".into(),
                context.into(),
                "-n".into(),
                namespace.clone().into(),
                "wait".into(),
                "--for=condition=Ready".into(),
                format!("pod/{pod_name}").into(),
                timeout_argument.clone().into(),
            ],
            [],
        )
        .with_context(|| format!("{pod_name} did not receive a ready NVIDIA GPU allocation"))?;
    }

    let mut allocated_uuids = BTreeSet::new();
    for pod_name in pod_names {
        let gpu = parse_single_nvidia_smi_gpu(&kubectl_exec(
            context,
            &namespace,
            pod_name,
            [
                "nvidia-smi",
                "--query-gpu=name,uuid,driver_version",
                "--format=csv,noheader",
            ],
        )?)?;
        let allocated_uuid = NvidiaGpuUuid::from_visible_devices(&kubectl_exec(
            context,
            &namespace,
            pod_name,
            ["printenv", "NVIDIA_VISIBLE_DEVICES"],
        )?)?;
        ensure!(
            allocated_uuid == gpu.uuid,
            "{pod_name} saw GPU {} but the device plugin allocated {}",
            gpu.uuid.as_str(),
            allocated_uuid.as_str()
        );
        println!(
            "{pod_name} on {node}: {} {} with driver {}",
            gpu.name,
            gpu.uuid.as_str(),
            gpu.driver_version
        );
        allocated_uuids.insert(gpu.uuid);
    }
    ensure!(
        allocated_uuids.len() == pod_names.len(),
        "two simultaneous one-GPU pods did not receive distinct physical GPU UUIDs: {allocated_uuids:?}"
    );

    namespace_guard.delete()?;
    println!(
        "GPU allocation isolation ok: two one-GPU pods on {node} received distinct device-plugin UUIDs"
    );
    Ok(())
}

fn ensure_digest_pinned_image(image: &str) -> Result<()> {
    let (_, digest) = image
        .rsplit_once("@sha256:")
        .context("GPU probe image must be pinned by sha256 digest")?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "GPU probe image has an invalid sha256 digest: {image}"
    );
    Ok(())
}

fn verify_node_capacity(context: &str, node: &str) -> Result<()> {
    let node_document: Value = serde_json::from_str(&run_checked(
        Path::new("kubectl"),
        [
            "--context".into(),
            context.into(),
            "get".into(),
            "node".into(),
            node.into(),
            "-o".into(),
            "json".into(),
        ],
        [],
    )?)?;
    let allocatable = node_document
        .pointer("/status/allocatable/nvidia.com~1gpu")
        .and_then(Value::as_str)
        .context("selected node does not advertise allocatable nvidia.com/gpu")?
        .parse::<u64>()
        .context("selected node advertised a non-integer nvidia.com/gpu quantity")?;
    ensure!(
        allocatable >= 2,
        "selected node must advertise at least two nvidia.com/gpu resources, found {allocatable}"
    );
    if let Some(sharing_strategy) = node_document
        .pointer("/metadata/labels/nvidia.com~1gpu.sharing-strategy")
        .and_then(Value::as_str)
    {
        ensure!(
            sharing_strategy == "none",
            "GPU allocation isolation rejects node sharing strategy {sharing_strategy:?}"
        );
    }
    Ok(())
}

fn gpu_probe_pod(
    namespace: &str,
    name: &str,
    node: &str,
    image: &str,
    runtime_class_name: &str,
) -> Value {
    serde_json::json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": name,
            "namespace": namespace,
            "labels": {
                "app.kubernetes.io/name": PROBE_PREFIX,
                "app.kubernetes.io/component": "gpu-allocation-probe",
            },
        },
        "spec": {
            "automountServiceAccountToken": false,
            "nodeName": node,
            "restartPolicy": "Never",
            "runtimeClassName": runtime_class_name,
            "terminationGracePeriodSeconds": 1,
            "containers": [{
                "name": "probe",
                "image": image,
                "imagePullPolicy": "IfNotPresent",
                "command": ["/bin/bash", "-ceu", "sleep infinity"],
                "env": [{
                    "name": "NVIDIA_DRIVER_CAPABILITIES",
                    "value": "compute,utility",
                }],
                "resources": {
                    "requests": {
                        "cpu": "10m",
                        "memory": "64Mi",
                        (GPU_RESOURCE): "1",
                    },
                    "limits": {
                        "cpu": "1",
                        "memory": "256Mi",
                        (GPU_RESOURCE): "1",
                    },
                },
                "securityContext": {
                    "allowPrivilegeEscalation": false,
                    "capabilities": {"drop": ["ALL"]},
                    "readOnlyRootFilesystem": true,
                },
            }],
        },
    })
}

fn kubectl_apply_value(context: &str, value: &Value) -> Result<()> {
    let mut child = Command::new("kubectl")
        .args(["--context", context, "apply", "-f", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .context("spawning kubectl apply for GPU allocation probes")?;
    serde_json::to_writer(
        child
            .stdin
            .as_mut()
            .context("kubectl stdin is unavailable")?,
        value,
    )?;
    child
        .stdin
        .take()
        .context("kubectl stdin is unavailable")?
        .flush()?;
    let status = child
        .wait()
        .context("waiting for GPU allocation probe apply")?;
    ensure!(
        status.success(),
        "kubectl apply for GPU allocation probes failed with {status}"
    );
    Ok(())
}

fn kubectl_exec<const N: usize>(
    context: &str,
    namespace: &str,
    pod: &str,
    command: [&str; N],
) -> Result<String> {
    let mut arguments = vec![
        "--context".into(),
        context.into(),
        "-n".into(),
        namespace.into(),
        "exec".into(),
        pod.into(),
        "-c".into(),
        "probe".into(),
        "--".into(),
    ];
    arguments.extend(command.into_iter().map(OsString::from));
    run_checked(Path::new("kubectl"), arguments, [])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_image_must_be_immutable() {
        assert!(ensure_digest_pinned_image("registry.example/probe:latest").is_err());
        assert!(
            ensure_digest_pinned_image(&format!(
                "registry.example/probe@sha256:{}",
                "a".repeat(64)
            ))
            .is_ok()
        );
    }

    #[test]
    fn probe_pod_leaves_visibility_to_the_device_plugin() {
        let image = format!("registry.example/probe@sha256:{}", "a".repeat(64));
        let pod = gpu_probe_pod("probe", "probe-a", "gpu-node", &image, "nvidia");
        assert_eq!(
            pod.pointer("/spec/containers/0/resources/requests/nvidia.com~1gpu"),
            Some(&Value::String("1".to_owned()))
        );
        assert_eq!(
            pod.pointer("/spec/containers/0/resources/limits/nvidia.com~1gpu"),
            Some(&Value::String("1".to_owned()))
        );
        assert_eq!(
            pod.pointer("/spec/nodeName").and_then(Value::as_str),
            Some("gpu-node")
        );
        let environment = pod
            .pointer("/spec/containers/0/env")
            .and_then(Value::as_array)
            .unwrap();
        assert!(environment.iter().all(|entry| {
            entry.pointer("/name").and_then(Value::as_str) != Some("NVIDIA_VISIBLE_DEVICES")
        }));
    }
}
