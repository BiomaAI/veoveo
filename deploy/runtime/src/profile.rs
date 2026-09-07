use crate::{
    charts::{helm_up, validate_helm_releases},
    cluster::{apply_local_cluster_bootstrap, wait_for_cluster_gpu, wait_for_cluster_nodes},
    configuration::{
        after_secret_closure, apply_config_map, apply_gateway_activation,
        prepare_gateway_activation, prepare_secret_closure,
        validate_node_bootstrap_secret_boundary,
    },
    gpu::{apply_gpu_placement, ensure_gpu_allocator, prepare_gpu_placement, verify_gpu_placement},
    images::{validate_bake_selections, validate_locked_images},
    process::{kubectl_apply_value, path_str, status_checked},
    sources::{
        load_deployment_lock, load_profile, resolve_locked_sources, resolve_sources,
        validate_locked_profile,
    },
};
use anyhow::{Context, Result};
use std::{path::Path, process::Command, time::Duration};
use veoveo_deploy_contract::DeploymentSourceRole;

pub fn profile_validate(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    validate_node_bootstrap_secret_boundary(&profile)?;
    let _gateway_activation = prepare_gateway_activation(&profile)?;
    let _gpu_placement = prepare_gpu_placement(&profile)?;
    let sources = resolve_sources(&profile)?;
    let selected_images = validate_bake_selections(&profile, &sources)?;
    profile.validate_image_plan(&selected_images)?;
    validate_helm_releases(&profile, &sources)?;
    let platform = profile.resolved_platform()?;
    println!(
        "Deployment profile {} is valid: {} sources, {} image publication phases, {} Helm releases, {} platform components, and {} MCP servers",
        profile.definition.name,
        sources.len(),
        sources
            .iter()
            .map(|source| match source.definition.role {
                DeploymentSourceRole::Platform => 1,
                DeploymentSourceRole::Extension | DeploymentSourceRole::Workload => {
                    source.definition.image_groups.len()
                }
            })
            .sum::<usize>(),
        sources
            .iter()
            .map(|source| source.definition.releases.len())
            .sum::<usize>(),
        platform.components.len(),
        platform.mcp_servers.len(),
    );
    Ok(())
}

pub fn profile_up(path: &Path, lock_path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let gateway_activation = prepare_gateway_activation(&profile)?;
    let lock = load_deployment_lock(lock_path)?;
    validate_locked_profile(&profile, &lock)?;
    let sources = resolve_locked_sources(&profile, &lock)?;
    let selected_images = validate_bake_selections(&profile, &sources)?;
    profile.validate_image_plan(&selected_images)?;
    validate_locked_images(&profile, &lock, &sources, &selected_images)?;
    validate_helm_releases(&profile, &sources)?;
    let platform = profile.resolved_platform()?;
    let context = profile.definition.kubernetes.context.as_str();
    let secret_closure = prepare_secret_closure(
        path,
        lock_path,
        &profile,
        &sources,
        &platform.components,
        &platform.mcp_servers,
        gateway_activation.as_ref(),
    )?;

    after_secret_closure(secret_closure, |_closure| {
        apply_local_cluster_bootstrap(&profile)?;
        if platform.gpu_scheduling.is_some() {
            wait_for_cluster_nodes(context, Duration::from_secs(120))?;
        } else {
            wait_for_cluster_gpu(context, Duration::from_secs(120))?;
        }

        kubectl_apply_value(
            context,
            &serde_json::json!({
                "apiVersion": "v1",
                "kind": "Namespace",
                "metadata": {"name": profile.definition.namespace}
            }),
        )?;

        if let Some(placement) = prepare_gpu_placement(&profile)? {
            let scheduling = platform
                .gpu_scheduling
                .as_ref()
                .context("prepared GPU placement has no resolved scheduling profile")?;
            ensure_gpu_allocator(context, &profile.definition.namespace, scheduling)?;
            apply_gpu_placement(
                context,
                &profile.definition.namespace,
                scheduling,
                &placement,
            )?;
        }

        for manifest in &profile.definition.resources.manifests {
            let manifest = profile.resolve(manifest);
            status_checked(
                "kubectl",
                [
                    "--context",
                    context,
                    "--namespace",
                    profile.definition.namespace.as_str(),
                    "apply",
                    "-f",
                    path_str(&manifest)?,
                ],
                &[],
                None,
            )?;
        }
        for config_map in &profile.definition.resources.config_maps {
            apply_config_map(&profile, context, config_map)?;
        }
        if let Some(activation) = &gateway_activation {
            apply_gateway_activation(context, &profile.definition.namespace, activation)?;
        }

        for source in &sources {
            for release in &source.definition.releases {
                helm_up(
                    &profile,
                    source,
                    context,
                    release,
                    &platform.components,
                    &platform.mcp_servers,
                )?;
            }
        }
        for deployment in &profile.definition.wait_for_deployments {
            let target = format!("deployment/{deployment}");
            status_checked(
                "kubectl",
                [
                    "--context",
                    context,
                    "--namespace",
                    profile.definition.namespace.as_str(),
                    "rollout",
                    "status",
                    target.as_str(),
                    "--timeout=10m",
                ],
                &[],
                None,
            )?;
        }
        if let Some(scheduling) = &platform.gpu_scheduling {
            verify_gpu_placement(context, &profile.definition.namespace, scheduling)?;
        }
        println!(
            "Deployment profile {} now runs {} digest-locked sources",
            profile.definition.name,
            sources.len()
        );
        Ok(())
    })
}

pub fn profile_gpu_verify(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let platform = profile.resolved_platform()?;
    let scheduling = platform
        .gpu_scheduling
        .as_ref()
        .context("deployment profile does not declare managed GPU scheduling")?;
    verify_gpu_placement(
        profile.definition.kubernetes.context.as_str(),
        profile.definition.namespace.as_str(),
        scheduling,
    )?;
    println!(
        "Deployment profile {} GPU placement is healthy",
        profile.definition.name
    );
    Ok(())
}

pub fn profile_down(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let context = profile.definition.kubernetes.context.as_str();
    let releases = profile
        .definition
        .sources
        .iter()
        .flat_map(|source| source.releases.iter())
        .collect::<Vec<_>>();
    for release in releases.into_iter().rev() {
        let output = Command::new("helm")
            .args([
                "--kube-context",
                context,
                "status",
                release.name.as_str(),
                "--namespace",
                profile.definition.namespace.as_str(),
            ])
            .output()
            .context("checking Helm release state")?;
        if output.status.success() {
            status_checked(
                "helm",
                [
                    "--kube-context",
                    context,
                    "uninstall",
                    release.name.as_str(),
                    "--namespace",
                    profile.definition.namespace.as_str(),
                ],
                &[],
                None,
            )?;
        }
    }
    Ok(())
}
