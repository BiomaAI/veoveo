use crate::{
    configuration::validate_node_bootstrap_secret_boundary,
    process::{output_checked, path_str, status_checked},
    sources::load_profile,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};
use veoveo_deploy_contract::{LoadedProfile, load_local_registry};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct K3dClusterSummary {
    name: String,
    servers_running: u64,
    servers_count: u64,
    agents_running: u64,
    agents_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct K3dRegistryState {
    #[serde(rename = "Running")]
    running: bool,
}

#[derive(Debug, Deserialize)]
struct K3dRegistrySummary {
    name: String,
    #[serde(rename = "State")]
    state: K3dRegistryState,
}

pub fn profile_registry_up(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    ensure_local_registry(&profile)
}

pub fn profile_cluster_up(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    validate_node_bootstrap_secret_boundary(&profile)?;
    ensure_local_registry(&profile)?;
    let cluster = profile
        .definition
        .kubernetes
        .local_cluster
        .as_ref()
        .context("deployment profile does not manage a local k3d cluster")?;
    let clusters = k3d_clusters()?;
    match clusters
        .iter()
        .find(|candidate| candidate.name == cluster.name)
    {
        Some(existing)
            if existing.servers_running == existing.servers_count
                && existing.agents_running == existing.agents_count =>
        {
            println!("k3d cluster {} is already running", cluster.name);
        }
        Some(_) => {
            status_checked(
                "k3d",
                ["cluster", "start", cluster.name.as_str()],
                &[],
                None,
            )?;
        }
        None => {
            let arguments = local_cluster_create_arguments(&profile)?;
            let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
            status_checked("k3d", arguments, &[], None)?;
        }
    }
    apply_local_cluster_bootstrap(&profile)?;
    if profile.resolved_platform()?.gpu_scheduling.is_some() {
        wait_for_cluster_nodes(
            &profile.definition.kubernetes.context,
            Duration::from_secs(120),
        )?;
    } else {
        wait_for_cluster_gpu(
            &profile.definition.kubernetes.context,
            Duration::from_secs(120),
        )?;
    }
    println!(
        "Deployment profile {} cluster is ready",
        profile.definition.name
    );
    Ok(())
}

pub fn profile_cluster_stop(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let cluster = profile
        .definition
        .kubernetes
        .local_cluster
        .as_ref()
        .context("deployment profile does not manage a local k3d cluster")?;
    if k3d_clusters()?.iter().any(|item| item.name == cluster.name) {
        status_checked("k3d", ["cluster", "stop", cluster.name.as_str()], &[], None)?;
    }
    Ok(())
}

pub fn profile_cluster_delete(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let cluster = profile
        .definition
        .kubernetes
        .local_cluster
        .as_ref()
        .context("deployment profile does not manage a local k3d cluster")?;
    if k3d_clusters()?.iter().any(|item| item.name == cluster.name) {
        status_checked(
            "k3d",
            ["cluster", "delete", cluster.name.as_str()],
            &[],
            None,
        )?;
    }
    Ok(())
}

pub(crate) fn ensure_local_registry(profile: &LoadedProfile) -> Result<()> {
    let config = profile
        .definition
        .registry
        .local_config
        .as_ref()
        .context("deployment profile does not manage a local registry")?;
    let registry = load_local_registry(&profile.resolve(config))?;
    let expected_name = registry.container_name();
    let registries = k3d_registries()?;
    if let Some(existing) = registries.iter().find(|item| item.name == expected_name) {
        ensure!(
            existing.state.running,
            "local registry {expected_name} is not running"
        );
        println!("Local registry {expected_name} is already running");
        return Ok(());
    }

    let mut args = vec![
        "registry".to_owned(),
        "create".to_owned(),
        registry.name.clone(),
        "--port".to_owned(),
        registry.host_port.clone(),
        "--image".to_owned(),
        registry.image.clone(),
        "--volume".to_owned(),
        registry.volume.clone(),
    ];
    if registry.delete_enabled {
        args.push("--delete-enabled".to_owned());
    }
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    status_checked("k3d", refs, &[], None)?;
    Ok(())
}

fn k3d_clusters() -> Result<Vec<K3dClusterSummary>> {
    let output = output_checked("k3d", ["cluster", "list", "-o", "json"], None)?;
    serde_json::from_slice(&output).context("decoding k3d cluster inventory")
}

fn k3d_registries() -> Result<Vec<K3dRegistrySummary>> {
    let output = output_checked("k3d", ["registry", "list", "-o", "json"], None)?;
    serde_json::from_slice(&output).context("decoding k3d registry inventory")
}

pub(crate) fn apply_local_cluster_bootstrap(profile: &LoadedProfile) -> Result<()> {
    let Some(cluster) = &profile.definition.kubernetes.local_cluster else {
        return Ok(());
    };
    for manifest in &cluster.node_bootstrap_manifests {
        let manifest = profile.resolve(manifest);
        status_checked(
            "kubectl",
            [
                "--context",
                profile.definition.kubernetes.context.as_str(),
                "apply",
                "-f",
                path_str(&manifest)?,
            ],
            &[],
            None,
        )?;
    }
    Ok(())
}

pub(crate) fn local_cluster_create_arguments(profile: &LoadedProfile) -> Result<Vec<String>> {
    let cluster = profile
        .definition
        .kubernetes
        .local_cluster
        .as_ref()
        .context("deployment profile does not manage a local k3d cluster")?;
    let config = profile.resolve(&cluster.config);
    let mut arguments = vec![
        "cluster".to_owned(),
        "create".to_owned(),
        "--config".to_owned(),
        path_str(&config)?.to_owned(),
    ];
    let mut destinations = std::collections::BTreeSet::new();
    for manifest in &cluster.node_bootstrap_manifests {
        let source = profile.resolve(manifest);
        let filename = source
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .with_context(|| {
                format!(
                    "node bootstrap manifest has no UTF-8 file name: {}",
                    source.display()
                )
            })?;
        ensure!(
            destinations.insert(filename.to_owned()),
            "node bootstrap manifests must have unique file names; duplicate `{filename}`"
        );
        arguments.push("--volume".to_owned());
        arguments.push(format!(
            "{}:/var/lib/rancher/k3s/server/manifests/{filename}@server:*",
            path_str(&source)?
        ));
    }
    Ok(arguments)
}

pub(crate) fn wait_for_cluster_gpu(context: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if cluster_gpu_capacity(context)? > 0 {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "Kubernetes context {context} exposes no allocatable NVIDIA GPU after {} seconds",
            timeout.as_secs()
        );
        thread::sleep(Duration::from_secs(1));
    }
}

pub(crate) fn wait_for_cluster_nodes(context: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let output = output_checked(
            "kubectl",
            ["--context", context, "get", "nodes", "-o", "json"],
            None,
        )?;
        let inventory =
            serde_json::from_slice::<Value>(&output).context("decoding node inventory")?;
        let ready = inventory["items"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|node| {
                node.pointer("/status/conditions")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|condition| {
                        condition.get("type").and_then(Value::as_str) == Some("Ready")
                            && condition.get("status").and_then(Value::as_str) == Some("True")
                    })
            });
        if ready {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "Kubernetes context {context} has no Ready node after {} seconds",
            timeout.as_secs()
        );
        thread::sleep(Duration::from_secs(1));
    }
}

pub(crate) fn cluster_gpu_capacity(context: &str) -> Result<u64> {
    let output = output_checked(
        "kubectl",
        ["--context", context, "get", "nodes", "-o", "json"],
        None,
    )?;
    let inventory = serde_json::from_slice::<Value>(&output).context("decoding node inventory")?;
    let gpu_capacity = inventory["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            node.pointer("/status/allocatable/nvidia.com~1gpu")?
                .as_str()
        })
        .filter_map(|capacity| capacity.parse::<u64>().ok())
        .sum::<u64>();
    Ok(gpu_capacity)
}
