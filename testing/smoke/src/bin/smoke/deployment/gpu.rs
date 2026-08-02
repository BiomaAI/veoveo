use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use veoveo_deploy_contract::{
    ConflictingGpuDevicePluginRemoval, GpuIsolation, GpuSchedulingProfile, GpuTimeSliceInterval,
    LoadedProfile, ManagedGpuAllocatorInstallation, NVIDIA_DRA_CONTAINER_TOOLKIT_PACKAGE_VERSION,
    NVIDIA_DRA_DRIVER_NAME, NVIDIA_DRA_HELM_VERSION, NVIDIA_DRA_HOST_DRIVER_VERSION,
    NVIDIA_DRA_KUBERNETES_VERSION,
};

use super::{kubectl_apply_value, output_checked, status_checked};

const MANAGED_NODE_LABEL: &str = "nvidia.com/dra-kubelet-plugin";
const MANAGED_NODE_LABEL_VALUE: &str = "true";

#[derive(Debug)]
pub(super) struct PreparedGpuPlacement {
    pub(super) claim_name: String,
    pub(super) runtime_class_name: String,
    pub(super) evidence_digest: String,
    pub(super) workload_requests: BTreeMap<String, String>,
    pub(super) workload_replicas: BTreeMap<String, u16>,
    pub(super) manifest: Value,
}

pub(super) fn prepare_gpu_placement(
    profile: &LoadedProfile,
) -> Result<Option<PreparedGpuPlacement>> {
    let platform = profile.resolved_platform()?;
    let Some(scheduling) = platform.gpu_scheduling else {
        return Ok(None);
    };
    Ok(Some(compile_gpu_placement(
        &profile.definition.name,
        &profile.definition.namespace,
        &scheduling,
    )?))
}

fn compile_gpu_placement(
    installation_name: &str,
    namespace: &str,
    scheduling: &GpuSchedulingProfile,
) -> Result<PreparedGpuPlacement> {
    let mut requests = Vec::new();
    let mut configuration = Vec::new();
    let mut workload_requests = BTreeMap::new();
    let mut workload_replicas = BTreeMap::new();
    for group in &scheduling.same_physical_device_groups {
        for workload in &group.workloads {
            workload_requests.insert(workload.workload.clone(), group.name.clone());
            workload_replicas.insert(workload.workload.clone(), workload.replicas);
        }
        let mut exactly = serde_json::json!({
            "deviceClassName": match group.isolation {
                GpuIsolation::Mig => &scheduling.allocator.mig_device_class_name,
                GpuIsolation::Exclusive | GpuIsolation::MeasuredTimeSlicing => {
                    &scheduling.allocator.full_device_class_name
                }
            },
            "allocationMode": "ExactCount",
            "count": 1
        });
        if let Some(profile) = &group.mig_profile {
            exactly["selectors"] = serde_json::json!([{
                "cel": {
                    "expression": format!(
                        "device.driver == 'gpu.nvidia.com' && device.attributes['gpu.nvidia.com'].type == 'mig' && device.attributes['gpu.nvidia.com'].profile == '{}'",
                        profile
                    )
                }
            }]);
        }
        requests.push(serde_json::json!({
            "name": group.name,
            "exactly": exactly
        }));
        if group.isolation == GpuIsolation::MeasuredTimeSlicing {
            let interval = match group
                .time_slice_interval
                .context("validated measured time-slicing group has no timeSliceInterval")?
            {
                GpuTimeSliceInterval::Short => "Short",
                GpuTimeSliceInterval::Default => "Default",
                GpuTimeSliceInterval::Long => "Long",
            };
            configuration.push(serde_json::json!({
                "requests": [group.name],
                "opaque": {
                    "driver": scheduling.allocator.driver_name,
                    "parameters": {
                        "apiVersion": scheduling.allocator.configuration_api_version,
                        "kind": "GpuConfig",
                        "sharing": {
                            "strategy": "TimeSlicing",
                            "timeSlicingConfig": {"interval": interval}
                        }
                    }
                }
            }));
        }
    }

    let groups = scheduling
        .same_physical_device_groups
        .iter()
        .map(|group| (group.name.as_str(), group))
        .collect::<BTreeMap<_, _>>();
    let constraints = scheduling
        .different_physical_device_groups
        .iter()
        .map(|constraint| {
            let mig = constraint.groups.iter().all(|name| {
                groups
                    .get(name.as_str())
                    .is_some_and(|group| group.isolation == GpuIsolation::Mig)
            });
            serde_json::json!({
                "requests": constraint.groups,
                "distinctAttribute": if mig {
                    "gpu.nvidia.com/parentUUID"
                } else {
                    "gpu.nvidia.com/uuid"
                }
            })
        })
        .collect::<Vec<_>>();
    let manifest = serde_json::json!({
        "apiVersion": "resource.k8s.io/v1",
        "kind": "ResourceClaim",
        "metadata": {
            "name": scheduling.allocator.claim_name,
            "namespace": namespace,
            "labels": {
                "app.kubernetes.io/managed-by": "veoveo-profile",
                "veoveo.ai/installation": installation_name
            },
            "annotations": {
                "veoveo.ai/gpu-placement-evidence": scheduling.evidence_digest
            }
        },
        "spec": {
            "devices": {
                "requests": requests,
                "constraints": constraints,
                "config": configuration
            }
        }
    });
    Ok(PreparedGpuPlacement {
        claim_name: scheduling.allocator.claim_name.clone(),
        runtime_class_name: scheduling.runtime_class_name.clone(),
        evidence_digest: scheduling.evidence_digest.clone(),
        workload_requests,
        workload_replicas,
        manifest,
    })
}

pub(super) fn ensure_gpu_allocator(
    context: &str,
    workload_namespace: &str,
    scheduling: &GpuSchedulingProfile,
) -> Result<()> {
    let installation = &scheduling.allocator.installation;
    validate_kubernetes_version(context)?;
    validate_helm_version()?;
    status_checked(
        "kubectl",
        [
            "--context",
            context,
            "get",
            "runtimeclass",
            scheduling.runtime_class_name.as_str(),
        ],
        &[],
        None,
    )
    .with_context(|| {
        format!(
            "GPU runtimeClass {} is unavailable; configure the CDI-enabled NVIDIA runtime before profile-up",
            scheduling.runtime_class_name
        )
    })?;
    let nodes = select_eligible_nodes(context, installation)?;
    let chart = pull_and_verify_chart(installation)?;

    remove_conflicting_device_plugin(context, workload_namespace, scheduling, installation)?;
    label_managed_nodes(context, &nodes)?;
    ensure_no_conflicting_device_plugin(context, &nodes)?;
    install_allocator_chart(context, installation, &chart)?;
    verify_allocator_release(context, installation, &nodes)?;
    verify_device_class(context, &scheduling.allocator.full_device_class_name, true)?;
    verify_resource_slices(
        context,
        &nodes,
        scheduling.allocatable_devices,
        Duration::from_secs(installation.timeout_seconds),
    )?;
    Ok(())
}

fn validate_kubernetes_version(context: &str) -> Result<()> {
    let output = output_checked(
        "kubectl",
        ["--context", context, "version", "-o", "json"],
        None,
    )?;
    let version: Value = serde_json::from_slice(&output).context("decoding Kubernetes version")?;
    let raw = version
        .pointer("/serverVersion/gitVersion")
        .and_then(Value::as_str)
        .context("Kubernetes server version has no gitVersion")?;
    let parsed = parse_version(raw).with_context(|| format!("parsing Kubernetes version {raw}"))?;
    let qualified = parse_version(NVIDIA_DRA_KUBERNETES_VERSION)
        .expect("qualified Kubernetes version constant is valid");
    ensure!(
        parsed == qualified,
        "managed NVIDIA DRA is qualified for Kubernetes {NVIDIA_DRA_KUBERNETES_VERSION}; context {context} runs {raw}"
    );
    Ok(())
}

fn validate_helm_version() -> Result<()> {
    let output = output_checked("helm", ["version", "--template", "{{.Version}}"], None)?;
    let raw = String::from_utf8(output)?.trim().to_owned();
    let parsed = parse_version(&raw).with_context(|| format!("parsing Helm version {raw}"))?;
    let qualified =
        parse_version(NVIDIA_DRA_HELM_VERSION).expect("qualified Helm version constant is valid");
    ensure!(
        parsed == qualified,
        "managed NVIDIA DRA is qualified for Helm {NVIDIA_DRA_HELM_VERSION}; installed version is {raw}"
    );
    Ok(())
}

fn select_eligible_nodes(
    context: &str,
    installation: &ManagedGpuAllocatorInstallation,
) -> Result<Vec<String>> {
    let selector = installation
        .eligible_node_selector
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    let output = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "get",
            "nodes",
            "-l",
            selector.as_str(),
            "-o",
            "json",
        ],
        None,
    )?;
    let inventory: Value =
        serde_json::from_slice(&output).context("decoding eligible GPU nodes")?;
    let items = inventory["items"]
        .as_array()
        .context("eligible GPU node inventory has no items")?;
    ensure!(
        !items.is_empty(),
        "GPU allocator eligibleNodeSelector {selector:?} matched no nodes"
    );
    let mut names = Vec::with_capacity(items.len());
    for node in items {
        let name = node
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .context("eligible GPU node has no name")?;
        let ready = node
            .pointer("/status/conditions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|condition| {
                condition.get("type").and_then(Value::as_str) == Some("Ready")
                    && condition.get("status").and_then(Value::as_str) == Some("True")
            });
        ensure!(ready, "GPU allocator eligible node {name} is not Ready");
        ensure!(
            !node
                .pointer("/spec/unschedulable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            "GPU allocator eligible node {name} is unschedulable"
        );
        names.push(name.to_owned());
    }
    names.sort();
    Ok(names)
}

fn pull_and_verify_chart(installation: &ManagedGpuAllocatorInstallation) -> Result<VerifiedChart> {
    let directory = tempfile::Builder::new()
        .prefix("veoveo-nvidia-dra-")
        .tempdir()
        .context("creating NVIDIA DRA chart verification directory")?;
    let output = Command::new("helm")
        .args([
            "pull",
            installation.chart.coordinate.as_str(),
            "--version",
            installation.chart.version.as_str(),
            "--destination",
            path_str(directory.path())?,
        ])
        .output()
        .context("pulling locked NVIDIA DRA chart")?;
    ensure!(
        output.status.success(),
        "Helm failed to pull locked NVIDIA DRA chart with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        report.contains(&format!("Digest: {}", installation.chart.digest)),
        "Helm pulled NVIDIA DRA chart without the locked OCI manifest digest {}; output was:\n{report}",
        installation.chart.digest
    );
    let archive = directory.path().join(format!(
        "dra-driver-nvidia-gpu-{}.tgz",
        installation.chart.version
    ));
    let bytes = fs::read(&archive)
        .with_context(|| format!("reading pulled NVIDIA DRA chart {}", archive.display()))?;
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
    ensure!(
        digest == installation.chart.content_digest,
        "NVIDIA DRA chart archive digest is {digest}, expected {}",
        installation.chart.content_digest
    );
    Ok(VerifiedChart {
        archive,
        _directory: directory,
    })
}

struct VerifiedChart {
    archive: PathBuf,
    _directory: tempfile::TempDir,
}

fn remove_conflicting_device_plugin(
    context: &str,
    workload_namespace: &str,
    scheduling: &GpuSchedulingProfile,
    installation: &ManagedGpuAllocatorInstallation,
) -> Result<()> {
    match &installation.conflicting_device_plugin_removal {
        ConflictingGpuDevicePluginRemoval::RequireAbsent => Ok(()),
        ConflictingGpuDevicePluginRemoval::DeleteDaemonSet { namespace, name } => {
            if !kubernetes_resource_exists(context, namespace, "daemonset", name)? {
                return Ok(());
            }
            quiesce_gpu_workloads(context, workload_namespace, scheduling)?;
            status_checked(
                "kubectl",
                [
                    "--context",
                    context,
                    "--namespace",
                    namespace,
                    "delete",
                    "daemonset",
                    name,
                    "--wait=true",
                    "--timeout=5m",
                ],
                &[],
                None,
            )
            .context("removing declared conflicting NVIDIA device-plugin DaemonSet")
        }
        ConflictingGpuDevicePluginRemoval::UninstallHelmRelease {
            namespace,
            release_name,
            expected_chart_version,
        } => {
            let output = Command::new("helm")
                .args([
                    "--kube-context",
                    context,
                    "status",
                    release_name,
                    "--namespace",
                    namespace,
                    "-o",
                    "json",
                ])
                .output()
                .context("checking declared conflicting NVIDIA device-plugin Helm release")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("release: not found") || stderr.contains("not found") {
                    return Ok(());
                }
                bail!(
                    "Helm failed while checking conflicting NVIDIA device-plugin release {namespace}/{release_name} with {}: {}",
                    output.status,
                    stderr.trim()
                );
            }
            let release: Value = serde_json::from_slice(&output.stdout)
                .context("decoding conflicting NVIDIA device-plugin Helm release")?;
            let chart = release
                .get("chart")
                .and_then(Value::as_str)
                .context("conflicting NVIDIA device-plugin Helm status omits chart")?;
            ensure!(
                chart.ends_with(&format!("-{expected_chart_version}")),
                "conflicting NVIDIA device-plugin release {namespace}/{release_name} runs chart {chart}, expected version {expected_chart_version}"
            );
            quiesce_gpu_workloads(context, workload_namespace, scheduling)?;
            status_checked(
                "helm",
                [
                    "--kube-context",
                    context,
                    "uninstall",
                    release_name,
                    "--namespace",
                    namespace,
                    "--wait",
                ],
                &[],
                None,
            )
            .context("uninstalling declared conflicting NVIDIA device-plugin Helm release")
        }
    }
}

fn kubernetes_resource_exists(
    context: &str,
    namespace: &str,
    kind: &str,
    name: &str,
) -> Result<bool> {
    let output = Command::new("kubectl")
        .args([
            "--context",
            context,
            "--namespace",
            namespace,
            "get",
            kind,
            name,
            "-o",
            "name",
        ])
        .output()
        .with_context(|| format!("checking Kubernetes {kind} {namespace}/{name}"))?;
    if output.status.success() {
        return Ok(true);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("NotFound") || stderr.contains("not found") {
        return Ok(false);
    }
    bail!(
        "kubectl failed while checking {kind} {namespace}/{name} with {}: {}",
        output.status,
        stderr.trim()
    )
}

fn quiesce_gpu_workloads(
    context: &str,
    namespace: &str,
    scheduling: &GpuSchedulingProfile,
) -> Result<()> {
    let deployments = scheduling
        .same_physical_device_groups
        .iter()
        .flat_map(|group| group.workloads.iter().map(|workload| &workload.deployment))
        .collect::<BTreeSet<_>>();
    for deployment in deployments {
        if !kubernetes_resource_exists(context, namespace, "deployment", deployment)? {
            continue;
        }
        status_checked(
            "kubectl",
            [
                "--context",
                context,
                "--namespace",
                namespace,
                "scale",
                "deployment",
                deployment,
                "--replicas=0",
            ],
            &[],
            None,
        )?;
        status_checked(
            "kubectl",
            [
                "--context",
                context,
                "--namespace",
                namespace,
                "rollout",
                "status",
                format!("deployment/{deployment}").as_str(),
                "--timeout=5m",
            ],
            &[],
            None,
        )?;
    }
    Ok(())
}

fn label_managed_nodes(context: &str, nodes: &[String]) -> Result<()> {
    for node in nodes {
        status_checked(
            "kubectl",
            [
                "--context",
                context,
                "label",
                "node",
                node,
                &format!("{MANAGED_NODE_LABEL}={MANAGED_NODE_LABEL_VALUE}"),
                "--overwrite",
            ],
            &[],
            None,
        )?;
    }
    Ok(())
}

fn ensure_no_conflicting_device_plugin(context: &str, nodes: &[String]) -> Result<()> {
    let output = output_checked(
        "kubectl",
        ["--context", context, "get", "pods", "-A", "-o", "json"],
        None,
    )?;
    let inventory: Value =
        serde_json::from_slice(&output).context("decoding cluster pod inventory")?;
    let conflicts = conflicting_device_plugin_pods(&inventory, nodes)?;
    ensure!(
        conflicts.is_empty(),
        "conflicting NVIDIA device-plugin pods still own DRA-selected nodes: {conflicts:?}; authorize their removal through conflictingDevicePluginRemoval before profile-up"
    );
    Ok(())
}

fn conflicting_device_plugin_pods(inventory: &Value, nodes: &[String]) -> Result<Vec<String>> {
    let selected = nodes.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let conflicts = inventory["items"]
        .as_array()
        .context("cluster pod inventory has no items")?
        .iter()
        .filter(|pod| {
            pod.pointer("/spec/nodeName")
                .and_then(Value::as_str)
                .is_some_and(|node| selected.contains(node))
        })
        .filter_map(|pod| {
            let namespace = pod.pointer("/metadata/namespace")?.as_str()?;
            let name = pod.pointer("/metadata/name")?.as_str()?;
            let conflicting = pod
                .pointer("/spec/containers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|container| {
                    let name = container.get("name").and_then(Value::as_str).unwrap_or("");
                    let image = container.get("image").and_then(Value::as_str).unwrap_or("");
                    (name.contains("device-plugin") || image.contains("k8s-device-plugin"))
                        && !image.contains("dra-driver-nvidia-gpu")
                });
            conflicting.then(|| format!("{namespace}/{name}"))
        })
        .collect::<Vec<_>>();
    Ok(conflicts)
}

fn allocator_helm_args(
    context: &str,
    installation: &ManagedGpuAllocatorInstallation,
    archive: &Path,
) -> Result<Vec<String>> {
    Ok(vec![
        "--kube-context".to_owned(),
        context.to_owned(),
        "upgrade".to_owned(),
        "--install".to_owned(),
        installation.release_name.clone(),
        path_str(archive)?.to_owned(),
        "--namespace".to_owned(),
        installation.namespace.clone(),
        "--create-namespace".to_owned(),
        "--atomic".to_owned(),
        "--wait".to_owned(),
        "--timeout".to_owned(),
        format!("{}s", installation.timeout_seconds),
        "--set-string".to_owned(),
        format!("nvidiaDriverRoot={}", installation.nvidia_driver_root),
        "--set".to_owned(),
        "gpuResourcesEnabledOverride=true".to_owned(),
        "--set-string".to_owned(),
        "resourceApiVersion=resource.k8s.io/v1".to_owned(),
        "--set".to_owned(),
        "resources.gpus.enabled=true".to_owned(),
        "--set".to_owned(),
        "resources.computeDomains.enabled=false".to_owned(),
        "--set".to_owned(),
        "featureGates.TimeSlicingSettings=true".to_owned(),
        "--set".to_owned(),
        "webhook.enabled=false".to_owned(),
        "--set-json".to_owned(),
        format!(
            "kubeletPlugin.nodeSelector={}",
            serde_json::to_string(&BTreeMap::from([(
                MANAGED_NODE_LABEL,
                MANAGED_NODE_LABEL_VALUE
            )]))?
        ),
        "--set-string".to_owned(),
        format!("image.repository={}", installation.image.repository),
        "--set-string".to_owned(),
        format!(
            "image.tag={}@{}",
            installation.image.tag, installation.image.digest
        ),
    ])
}

fn install_allocator_chart(
    context: &str,
    installation: &ManagedGpuAllocatorInstallation,
    chart: &VerifiedChart,
) -> Result<()> {
    let args = allocator_helm_args(context, installation, &chart.archive)?;
    status_checked("helm", args.iter().map(String::as_str), &[], None)
        .context("installing the locked NVIDIA DRA driver")
}

fn verify_allocator_release(
    context: &str,
    installation: &ManagedGpuAllocatorInstallation,
    nodes: &[String],
) -> Result<()> {
    let release = output_checked(
        "helm",
        [
            "--kube-context",
            context,
            "status",
            installation.release_name.as_str(),
            "--namespace",
            installation.namespace.as_str(),
            "-o",
            "json",
        ],
        None,
    )?;
    let release: Value =
        serde_json::from_slice(&release).context("decoding NVIDIA DRA Helm release")?;
    let chart = release
        .get("chart")
        .and_then(Value::as_str)
        .context("NVIDIA DRA Helm status omits chart")?;
    ensure!(
        chart == format!("dra-driver-nvidia-gpu-{}", installation.chart.version),
        "NVIDIA DRA Helm release uses chart {chart}, expected {}",
        installation.chart.version
    );

    let daemon_sets = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--namespace",
            installation.namespace.as_str(),
            "get",
            "daemonsets",
            "-l",
            format!("app.kubernetes.io/instance={}", installation.release_name).as_str(),
            "-o",
            "json",
        ],
        None,
    )?;
    let daemon_sets: Value =
        serde_json::from_slice(&daemon_sets).context("decoding NVIDIA DRA DaemonSets")?;
    let expected_image = format!(
        "{}:{}@{}",
        installation.image.repository, installation.image.tag, installation.image.digest
    );
    let pods = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--namespace",
            installation.namespace.as_str(),
            "get",
            "pods",
            "-l",
            format!("app.kubernetes.io/instance={}", installation.release_name).as_str(),
            "-o",
            "json",
        ],
        None,
    )?;
    let pods: Value = serde_json::from_slice(&pods).context("decoding NVIDIA DRA pods")?;
    let mut container_count = 0_usize;
    for pod in pods["items"]
        .as_array()
        .context("NVIDIA DRA pod inventory has no items")?
    {
        for path in ["/spec/initContainers", "/spec/containers"] {
            for container in pod
                .pointer(path)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                container_count += 1;
                let name = container
                    .get("name")
                    .and_then(Value::as_str)
                    .context("NVIDIA DRA container has no name")?;
                let image = container
                    .get("image")
                    .and_then(Value::as_str)
                    .context("NVIDIA DRA container has no image")?;
                ensure!(
                    image == expected_image,
                    "NVIDIA DRA container {name} uses image {image}, expected {expected_image}"
                );
            }
        }
    }
    ensure!(
        container_count > 0,
        "NVIDIA DRA release has no running containers to verify"
    );
    verify_container_toolkit_versions(context, installation, &pods, nodes)?;
    let kubelet = daemon_sets["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|daemon_set| {
            daemon_set
                .pointer("/metadata/name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.contains("kubelet-plugin"))
        })
        .context("NVIDIA DRA release has no kubelet-plugin DaemonSet")?;
    let image = kubelet
        .pointer("/spec/template/spec/containers/0/image")
        .and_then(Value::as_str)
        .context("NVIDIA DRA kubelet-plugin has no image")?;
    ensure!(
        image == expected_image,
        "NVIDIA DRA kubelet-plugin image is {image}, expected {expected_image}"
    );
    let ready = kubelet
        .pointer("/status/numberReady")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    ensure!(
        ready == u64::try_from(nodes.len()).expect("node count fits u64"),
        "NVIDIA DRA kubelet-plugin has {ready} Ready pods for {} selected nodes",
        nodes.len()
    );
    Ok(())
}

fn verify_container_toolkit_versions(
    context: &str,
    installation: &ManagedGpuAllocatorInstallation,
    pods: &Value,
    nodes: &[String],
) -> Result<()> {
    for node in nodes {
        let pod = pods["items"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|pod| {
                pod.pointer("/spec/nodeName").and_then(Value::as_str) == Some(node.as_str())
                    && pod
                        .pointer("/spec/containers")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .any(|container| {
                            container.get("name").and_then(Value::as_str) == Some("gpus")
                        })
            })
            .with_context(|| format!("NVIDIA DRA has no GPU kubelet-plugin pod on node {node}"))?;
        let pod_name = pod
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .context("NVIDIA DRA kubelet-plugin pod has no name")?;
        let status = output_checked(
            "kubectl",
            [
                "--context",
                context,
                "--namespace",
                installation.namespace.as_str(),
                "exec",
                pod_name,
                "-c",
                "gpus",
                "--",
                "cat",
                "/driver-root/var/lib/dpkg/status",
            ],
            None,
        )
        .with_context(|| {
            format!(
                "reading the host NVIDIA Container Toolkit package inventory through node {node}"
            )
        })?;
        let status = String::from_utf8(status).context("decoding host package inventory")?;
        let version = debian_package_version(&status, "nvidia-container-toolkit")
            .context("host package inventory omits installed nvidia-container-toolkit")?;
        ensure!(
            version == NVIDIA_DRA_CONTAINER_TOOLKIT_PACKAGE_VERSION,
            "node {node} runs NVIDIA Container Toolkit package {version}; the qualified installation pins {NVIDIA_DRA_CONTAINER_TOOLKIT_PACKAGE_VERSION}"
        );
    }
    Ok(())
}

fn debian_package_version<'a>(status: &'a str, package: &str) -> Option<&'a str> {
    status.split("\n\n").find_map(|paragraph| {
        let mut name = None;
        let mut version = None;
        let mut installed = false;
        for line in paragraph.lines() {
            if let Some(value) = line.strip_prefix("Package: ") {
                name = Some(value);
            } else if let Some(value) = line.strip_prefix("Version: ") {
                version = Some(value);
            } else if line == "Status: install ok installed" {
                installed = true;
            }
        }
        (name == Some(package) && installed)
            .then_some(version)
            .flatten()
    })
}

fn verify_device_class(context: &str, name: &str, require_extended_resource: bool) -> Result<()> {
    let class = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "get",
            "deviceclass",
            name,
            "-o",
            "json",
        ],
        None,
    )?;
    let class: Value = serde_json::from_slice(&class).context("decoding NVIDIA DRA DeviceClass")?;
    if require_extended_resource {
        ensure!(
            class
                .pointer("/spec/extendedResourceName")
                .and_then(Value::as_str)
                == Some("nvidia.com/gpu"),
            "GPU DRA DeviceClass {name} does not expose the nvidia.com/gpu extended resource"
        );
    }
    let selects_driver = class
        .pointer("/spec/selectors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|selector| selector.pointer("/cel/expression").and_then(Value::as_str))
        .any(|expression| expression.contains(NVIDIA_DRA_DRIVER_NAME));
    ensure!(
        selects_driver,
        "GPU DRA DeviceClass {name} does not select driver {NVIDIA_DRA_DRIVER_NAME}"
    );
    Ok(())
}

fn verify_resource_slices(
    context: &str,
    nodes: &[String],
    expected_devices: u16,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let uuids = loop {
        let slices = output_checked(
            "kubectl",
            ["--context", context, "get", "resourceslices", "-o", "json"],
            None,
        )?;
        let slices: Value =
            serde_json::from_slice(&slices).context("decoding NVIDIA DRA ResourceSlices")?;
        match validate_resource_slices(&slices, nodes, expected_devices) {
            Ok(uuids) => break uuids,
            Err(error) if Instant::now() < deadline => {
                eprintln!("waiting for complete NVIDIA DRA ResourceSlices: {error:#}");
                thread::sleep(Duration::from_secs(1));
            }
            Err(error) => {
                return Err(error).context(format!(
                    "NVIDIA DRA ResourceSlices did not become ready within {} seconds",
                    timeout.as_secs()
                ));
            }
        }
    };
    println!(
        "NVIDIA DRA ready: driver={} nodes={} physicalGpuUuids={}",
        NVIDIA_DRA_DRIVER_NAME,
        nodes.join(","),
        uuids.into_iter().collect::<Vec<_>>().join(",")
    );
    Ok(())
}

fn validate_resource_slices(
    slices: &Value,
    nodes: &[String],
    expected_devices: u16,
) -> Result<BTreeSet<String>> {
    let selected = nodes.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut uuids = BTreeSet::new();
    let mut seen_nodes = BTreeSet::new();
    for slice in slices["items"]
        .as_array()
        .context("ResourceSlice inventory has no items")?
    {
        if slice.pointer("/spec/driver").and_then(Value::as_str) != Some(NVIDIA_DRA_DRIVER_NAME) {
            continue;
        }
        let Some(node) = slice.pointer("/spec/nodeName").and_then(Value::as_str) else {
            continue;
        };
        if !selected.contains(node) {
            continue;
        }
        seen_nodes.insert(node);
        for device in slice
            .pointer("/spec/devices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let attributes = device
                .get("attributes")
                .and_then(Value::as_object)
                .context("NVIDIA DRA ResourceSlice device has no attributes")?;
            if attributes
                .get("type")
                .and_then(|value| value.get("string"))
                .and_then(Value::as_str)
                != Some("gpu")
            {
                continue;
            }
            let uuid = attributes
                .get("uuid")
                .and_then(|value| value.get("string"))
                .and_then(Value::as_str)
                .context("NVIDIA DRA physical GPU device has no UUID")?;
            let driver = attributes
                .get("driverVersion")
                .and_then(|value| value.get("version"))
                .and_then(Value::as_str)
                .context("NVIDIA DRA physical GPU device has no driverVersion")?;
            let driver = parse_version(driver)
                .with_context(|| format!("parsing NVIDIA driver version for GPU {uuid}"))?;
            let qualified = parse_version(NVIDIA_DRA_HOST_DRIVER_VERSION)
                .expect("qualified NVIDIA driver version constant is valid");
            ensure!(
                driver == qualified,
                "NVIDIA DRA GPU {uuid} exposes driver version {driver:?}; the qualified installation pins {NVIDIA_DRA_HOST_DRIVER_VERSION}"
            );
            ensure!(
                uuids.insert(uuid.to_owned()),
                "NVIDIA DRA ResourceSlices duplicate physical GPU UUID {uuid}"
            );
        }
    }
    ensure!(
        seen_nodes.len() == selected.len(),
        "NVIDIA DRA ResourceSlices cover nodes {seen_nodes:?}, expected {selected:?}"
    );
    ensure!(
        uuids.len() == usize::from(expected_devices),
        "NVIDIA DRA exposes {} unique physical GPUs on selected nodes, profile requires {expected_devices}",
        uuids.len()
    );
    Ok(uuids)
}

pub(super) fn apply_gpu_placement(
    context: &str,
    namespace: &str,
    scheduling: &GpuSchedulingProfile,
    placement: &PreparedGpuPlacement,
) -> Result<()> {
    let mut classes = BTreeMap::new();
    for group in &scheduling.same_physical_device_groups {
        let (class, require_extended_resource) = match group.isolation {
            GpuIsolation::Mig => (&scheduling.allocator.mig_device_class_name, false),
            GpuIsolation::Exclusive | GpuIsolation::MeasuredTimeSlicing => {
                (&scheduling.allocator.full_device_class_name, true)
            }
        };
        classes.insert(class, require_extended_resource);
    }
    for (class, require_extended_resource) in classes {
        verify_device_class(context, class, require_extended_resource)?;
    }
    kubectl_apply_value(context, &placement.manifest).with_context(|| {
        format!(
            "applying restart-stable GPU ResourceClaim {namespace}/{}",
            placement.claim_name
        )
    })
}

pub(super) fn verify_gpu_placement(
    context: &str,
    namespace: &str,
    scheduling: &GpuSchedulingProfile,
) -> Result<()> {
    let claim = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--namespace",
            namespace,
            "get",
            "resourceclaim",
            scheduling.allocator.claim_name.as_str(),
            "-o",
            "json",
        ],
        None,
    )?;
    let claim: Value = serde_json::from_slice(&claim).context("decoding GPU ResourceClaim")?;
    let allocated = claim
        .pointer("/status/allocation/devices/results")
        .and_then(Value::as_array)
        .context("GPU ResourceClaim has no allocated device results")?;
    let allocated_requests = allocated
        .iter()
        .filter_map(|item| item.get("request").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();

    let mut group_uuids = BTreeMap::<String, BTreeSet<String>>::new();
    for group in &scheduling.same_physical_device_groups {
        ensure!(
            allocated_requests.contains(group.name.as_str()),
            "GPU ResourceClaim did not allocate request {}",
            group.name
        );
        let uuids = group_uuids.entry(group.name.clone()).or_default();
        for workload in &group.workloads {
            let pods = output_checked(
                "kubectl",
                [
                    "--context",
                    context,
                    "--namespace",
                    namespace,
                    "get",
                    "pods",
                    "-l",
                    format!("app.kubernetes.io/component={}", workload.deployment).as_str(),
                    "-o",
                    "json",
                ],
                None,
            )?;
            let pods: Value =
                serde_json::from_slice(&pods).context("decoding GPU workload pods")?;
            let pod_names = pods["items"]
                .as_array()
                .context("GPU workload pod list has no items")?
                .iter()
                .filter_map(|pod| pod.pointer("/metadata/name").and_then(Value::as_str))
                .collect::<Vec<_>>();
            ensure!(
                pod_names.len() == usize::from(workload.replicas),
                "GPU workload {} expected {} replicas but found {} pods",
                workload.workload,
                workload.replicas,
                pod_names.len()
            );
            for pod in pod_names {
                let output = output_checked(
                    "kubectl",
                    [
                        "--context",
                        context,
                        "--namespace",
                        namespace,
                        "exec",
                        pod,
                        "-c",
                        workload.container.as_str(),
                        "--",
                        "nvidia-smi",
                        "--query-gpu=uuid",
                        "--format=csv,noheader,nounits",
                    ],
                    None,
                )?;
                let visible = String::from_utf8(output)?
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                ensure!(
                    visible.len() == 1,
                    "GPU workload {} pod {pod} sees {} devices; expected exactly one allocated UUID",
                    workload.workload,
                    visible.len()
                );
                println!(
                    "GPU placement ready: workload={} pod={} physicalUuid={} group={}",
                    workload.workload, pod, visible[0], group.name
                );
                uuids.insert(visible[0].clone());
            }
        }
        ensure!(
            uuids.len() == 1,
            "same-physical-device group {} resolved to different GPU UUIDs: {uuids:?}",
            group.name
        );
    }
    for constraint in &scheduling.different_physical_device_groups {
        let mut seen = BTreeSet::new();
        for group in &constraint.groups {
            let uuid = group_uuids
                .get(group)
                .and_then(|values| values.first())
                .with_context(|| format!("GPU group {group} has no verified UUID"))?;
            ensure!(
                seen.insert(uuid),
                "different-physical-device constraint drifted: groups {:?} share UUID {uuid}",
                constraint.groups
            );
        }
    }
    Ok(())
}

fn parse_version(raw: &str) -> Result<(u64, u64, u64)> {
    let raw = raw.trim().trim_start_matches('v');
    let numeric = raw
        .split(|character: char| !(character.is_ascii_digit() || character == '.'))
        .next()
        .unwrap_or(raw);
    let mut components = numeric.split('.');
    let major = components
        .next()
        .context("version has no major component")?
        .parse()?;
    let minor = components.next().unwrap_or("0").parse()?;
    let patch = components.next().unwrap_or("0").parse()?;
    Ok((major, minor, patch))
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::Path};

    use serde_json::json;
    use veoveo_deploy_contract::{
        GpuDifferentPhysicalDeviceConstraint, GpuWorkloadPlacement, LoadedProfile,
    };

    use super::{
        allocator_helm_args, compile_gpu_placement, conflicting_device_plugin_pods,
        debian_package_version, parse_version, prepare_gpu_placement, validate_resource_slices,
    };

    #[test]
    fn checked_in_profile_compiles_restart_stable_gpu_claim() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile_path =
            repository.join("testing/fixtures/external-simulation-installation/deployment.json");
        let profile = LoadedProfile::load(&profile_path, &repository).unwrap();
        let placement = prepare_gpu_placement(&profile).unwrap().unwrap();

        assert_eq!(placement.claim_name, "anonymous-simulation-gpu");
        assert_eq!(placement.manifest["apiVersion"], "resource.k8s.io/v1");
        assert_eq!(
            placement.manifest.pointer("/spec/devices/requests/0/name"),
            Some(&json!("simulation-view"))
        );
        assert_eq!(
            placement
                .workload_requests
                .get("simulation-view-renderer")
                .map(String::as_str),
            Some("simulation-view")
        );
    }

    #[test]
    fn compiles_two_named_requests_with_a_physical_uuid_constraint() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile_path =
            repository.join("testing/fixtures/external-simulation-installation/deployment.json");
        let profile = LoadedProfile::load(&profile_path, &repository).unwrap();
        let mut scheduling = profile.resolved_platform().unwrap().gpu_scheduling.unwrap();
        let mut second = scheduling.same_physical_device_groups[0].clone();
        second.name = "visualization".to_owned();
        second.workloads = vec![GpuWorkloadPlacement {
            workload: "view-renderer".to_owned(),
            deployment: "view-renderer".to_owned(),
            container: "view-isaac".to_owned(),
            replicas: 1,
        }];
        scheduling.allocatable_devices = 2;
        scheduling.same_physical_device_groups.push(second);
        scheduling.different_physical_device_groups = vec![GpuDifferentPhysicalDeviceConstraint {
            groups: BTreeSet::from(["simulation-view".to_owned(), "visualization".to_owned()]),
        }];

        let placement = compile_gpu_placement("example", "veoveo", &scheduling).unwrap();
        assert_eq!(
            placement.manifest.pointer("/spec/devices/requests/1/name"),
            Some(&json!("visualization"))
        );
        assert_eq!(
            placement
                .manifest
                .pointer("/spec/devices/constraints/0/distinctAttribute"),
            Some(&json!("gpu.nvidia.com/uuid"))
        );
    }

    #[test]
    fn allocator_helm_values_enable_v1_gpu_time_slicing_and_pin_image() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let profile_path =
            repository.join("testing/fixtures/external-simulation-installation/deployment.json");
        let profile = LoadedProfile::load(&profile_path, &repository).unwrap();
        let scheduling = profile.resolved_platform().unwrap().gpu_scheduling.unwrap();
        let args = allocator_helm_args(
            "example",
            &scheduling.allocator.installation,
            Path::new("/tmp/driver.tgz"),
        )
        .unwrap();
        let rendered = args.join(" ");

        assert!(rendered.contains("resourceApiVersion=resource.k8s.io/v1"));
        assert!(rendered.contains("featureGates.TimeSlicingSettings=true"));
        assert!(rendered.contains("resources.computeDomains.enabled=false"));
        assert!(rendered.contains("gpuResourcesEnabledOverride=true"));
        assert!(rendered.contains("@sha256:eefe6739"));
    }

    #[test]
    fn resource_slice_inventory_requires_distinct_physical_gpus_and_current_driver() {
        let inventory = json!({"items": [{
            "spec": {
                "driver": "gpu.nvidia.com",
                "nodeName": "gpu-node",
                "devices": [
                    {"attributes": {
                        "type": {"string": "gpu"},
                        "uuid": {"string": "GPU-a"},
                        "driverVersion": {"version": "610.43.02"}
                    }},
                    {"attributes": {
                        "type": {"string": "gpu"},
                        "uuid": {"string": "GPU-b"},
                        "driverVersion": {"version": "610.43.02"}
                    }}
                ]
            }
        }]});
        let nodes = vec!["gpu-node".to_owned()];

        assert_eq!(
            validate_resource_slices(&inventory, &nodes, 2).unwrap(),
            ["GPU-a".to_owned(), "GPU-b".to_owned()]
                .into_iter()
                .collect()
        );
        assert!(validate_resource_slices(&inventory, &nodes, 3).is_err());

        let mut stale = inventory;
        stale["items"][0]["spec"]["devices"][0]["attributes"]["driverVersion"]["version"] =
            json!("570.0.0");
        assert!(validate_resource_slices(&stale, &nodes, 2).is_err());
    }

    #[test]
    fn parses_kubernetes_and_driver_versions_without_changing_driver_pin() {
        assert_eq!(parse_version("v1.36.2+k3s1").unwrap(), (1, 36, 2));
        assert_eq!(parse_version("610.43.02").unwrap(), (610, 43, 2));
    }

    #[test]
    fn conflicting_device_plugin_is_rejected_without_mistaking_dra_for_it() {
        let inventory = json!({"items": [
            {
                "metadata": {"namespace": "kube-system", "name": "device-plugin"},
                "spec": {"nodeName": "gpu-node", "containers": [{
                    "name": "nvidia-device-plugin",
                    "image": "nvcr.io/nvidia/k8s-device-plugin:v0.19.3"
                }]}
            },
            {
                "metadata": {"namespace": "nvidia-dra-driver-gpu", "name": "dra"},
                "spec": {"nodeName": "gpu-node", "containers": [{
                    "name": "gpu-kubelet-plugin",
                    "image": "registry.k8s.io/dra-driver-nvidia/dra-driver-nvidia-gpu:v0.4.1"
                }]}
            }
        ]});

        assert_eq!(
            conflicting_device_plugin_pods(&inventory, &["gpu-node".to_owned()]).unwrap(),
            ["kube-system/device-plugin"]
        );
    }

    #[test]
    fn parses_exact_installed_container_toolkit_package() {
        let status = "Package: unrelated\nStatus: install ok installed\nVersion: 1.0\n\nPackage: nvidia-container-toolkit\nStatus: install ok installed\nVersion: 1.19.1-1\n";

        assert_eq!(
            debian_package_version(status, "nvidia-container-toolkit"),
            Some("1.19.1-1")
        );
        assert_eq!(
            debian_package_version(
                "Package: nvidia-container-toolkit\nStatus: deinstall ok config-files\nVersion: 1.19.1-1\n",
                "nvidia-container-toolkit"
            ),
            None
        );
    }
}
